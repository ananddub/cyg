using System.Collections.Generic;
using CookYourGame.Sim;

namespace CookYourGame.SportsArena
{
    /// <summary>Lifecycle of a two-client lockstep match.</summary>
    public enum SportsArenaLockstepState
    {
        Connecting,
        WaitingForPeer,
        Ready,
        Running,
        Stalled,
        Paused,
        Desync,
    }

    /// <summary>
    /// Tick-keyed input buffer and state-hash verifier for the two-client
    /// Sports Arena prototype.
    ///
    /// The CYG input hub relays opaque input packets but does not provide
    /// a shared tick epoch or server snapshots. Both clients therefore start at
    /// tick 0 through a small HELLO/READY/START handshake, keep a short input
    /// delay, and only advance a tick when both human inputs for that exact
    /// tick are present. Every <see cref="HashInterval"/> ticks each client
    /// exchanges a CRC of its deterministic simulation state.
    /// </summary>
    public sealed class SportsArenaLockstep
    {
        public const int DefaultInputDelay = 2;
        public const int HashInterval = 60;
        private const int InputCapacity = 256;
        private const int MaxInputAhead = 64;
        private const int HashHistory = 512;

        private readonly CYGInputRingBuffer _inputs = new CYGInputRingBuffer(InputCapacity, 4);
        private readonly Dictionary<int, uint> _localHashes = new Dictionary<int, uint>();
        private readonly Dictionary<int, uint> _remoteHashes = new Dictionary<int, uint>();
        private readonly List<int> _staleTicks = new List<int>(32);

        public SportsArenaLockstepState State { get; private set; } = SportsArenaLockstepState.Connecting;
        public int Tick { get; private set; }
        public int InputDelay { get; set; } = DefaultInputDelay;
        public int LocalActor { get; private set; }
        public int RemoteActor { get; private set; }
        public int LocalPlayerIndex { get; private set; }
        public int RemotePlayerIndex { get; private set; } = 1;
        public int StartEpoch { get; private set; }

        public bool HasDesync => State == SportsArenaLockstepState.Desync;
        public int DesyncTick { get; private set; } = -1;
        public uint DesyncLocalHash { get; private set; }
        public uint DesyncRemoteHash { get; private set; }
        public int MismatchCount { get; private set; }
        public int VerifiedHashCount { get; private set; }
        public int LastVerifiedTick { get; private set; } = -1;
        public uint LastLocalHash { get; private set; }
        public int LastLocalHashTick { get; private set; } = -1;
        public int LastRemoteInputTick { get; private set; } = -1;

        public void SetState(SportsArenaLockstepState state)
        {
            if (State == SportsArenaLockstepState.Desync && state != SportsArenaLockstepState.Desync)
            {
                return;
            }
            State = state;
        }

        /// <summary>
        /// Clears the input window and starts a fresh epoch at tick 0. Both
        /// clients call this when they accept the same START message.
        /// </summary>
        public void ResetForStart(
            int localActor,
            int remoteActor,
            int localPlayerIndex,
            int remotePlayerIndex,
            int epoch,
            int inputDelay)
        {
            LocalActor = localActor;
            RemoteActor = remoteActor;
            LocalPlayerIndex = localPlayerIndex;
            RemotePlayerIndex = remotePlayerIndex;
            StartEpoch = epoch;
            InputDelay = inputDelay < 0 ? 0 : inputDelay;
            Tick = 0;
            LastRemoteInputTick = -1;
            LastVerifiedTick = -1;
            LastLocalHashTick = -1;
            LastLocalHash = 0;
            DesyncTick = -1;
            DesyncLocalHash = 0;
            DesyncRemoteHash = 0;
            MismatchCount = 0;
            VerifiedHashCount = 0;

            _inputs.Clear();
            _inputs.GetOrAddPlayerIndex(localPlayerIndex);
            _inputs.GetOrAddPlayerIndex(remotePlayerIndex);
            _localHashes.Clear();
            _remoteHashes.Clear();
            State = SportsArenaLockstepState.Running;
        }

        public bool StoreLocal(int tick, in CYGPlayerInput input)
        {
            if (tick < 0 || State == SportsArenaLockstepState.Desync)
            {
                return false;
            }
            _inputs.SetInput(LocalPlayerIndex, tick, input);
            return true;
        }

        /// <summary>
        /// Stores a relayed input. Ticks far outside the current window are
        /// ignored so a malformed packet cannot grow the buffer without bound.
        /// </summary>
        public bool StoreRemote(int tick, in CYGPlayerInput input)
        {
            if (tick < 0
                || (State != SportsArenaLockstepState.Running && State != SportsArenaLockstepState.Stalled)
                || tick < Tick - InputCapacity
                || tick > Tick + MaxInputAhead)
            {
                return false;
            }

            _inputs.SetInput(RemotePlayerIndex, tick, input);
            if (tick > LastRemoteInputTick)
            {
                LastRemoteInputTick = tick;
            }
            return true;
        }

        public bool HasLocalInput(int tick)
        {
            return _inputs.TryGetInput(LocalPlayerIndex, tick, out _, out bool predicted) && !predicted;
        }

        public bool HasRemoteInput(int tick)
        {
            return _inputs.TryGetInput(RemotePlayerIndex, tick, out _, out bool predicted) && !predicted;
        }

        /// <summary>
        /// Returns true only when both human players have a confirmed input for
        /// the exact tick. There is deliberately no prediction in v1: waiting is
        /// safer than inventing a remote input and needing rollback.
        /// </summary>
        public bool TryGetTickInputs(int tick, out CYGPlayerInput local, out CYGPlayerInput remote)
        {
            bool hasLocal = _inputs.TryGetInput(LocalPlayerIndex, tick, out local, out bool localPredicted)
                && !localPredicted;
            bool hasRemote = _inputs.TryGetInput(RemotePlayerIndex, tick, out remote, out bool remotePredicted)
                && !remotePredicted;
            return hasLocal && hasRemote;
        }

        public void AdvanceTo(int nextTick)
        {
            if (nextTick > Tick)
            {
                Tick = nextTick;
                PruneHashes();
            }
        }

        public bool RecordLocalHash(int tick, uint hash)
        {
            LastLocalHashTick = tick;
            LastLocalHash = hash;
            if (_remoteHashes.TryGetValue(tick, out uint remote))
            {
                _remoteHashes.Remove(tick);
                return CompareHashes(tick, hash, remote);
            }

            _localHashes[tick] = hash;
            PruneHashes();
            return true;
        }

        public bool RecordRemoteHash(int tick, uint hash)
        {
            if (_localHashes.TryGetValue(tick, out uint local))
            {
                _localHashes.Remove(tick);
                return CompareHashes(tick, local, hash);
            }

            _remoteHashes[tick] = hash;
            PruneHashes();
            return true;
        }

        private bool CompareHashes(int tick, uint local, uint remote)
        {
            if (local == remote)
            {
                VerifiedHashCount++;
                LastVerifiedTick = tick;
                return true;
            }

            MismatchCount++;
            if (State != SportsArenaLockstepState.Desync)
            {
                DesyncTick = tick;
                DesyncLocalHash = local;
                DesyncRemoteHash = remote;
                State = SportsArenaLockstepState.Desync;
            }
            return false;
        }

        private void PruneHashes()
        {
            int minTick = Tick - HashHistory;
            if (minTick <= 0)
            {
                return;
            }

            _staleTicks.Clear();
            foreach (KeyValuePair<int, uint> entry in _localHashes)
            {
                if (entry.Key < minTick)
                {
                    _staleTicks.Add(entry.Key);
                }
            }
            for (int i = 0; i < _staleTicks.Count; i++)
            {
                _localHashes.Remove(_staleTicks[i]);
            }

            _staleTicks.Clear();
            foreach (KeyValuePair<int, uint> entry in _remoteHashes)
            {
                if (entry.Key < minTick)
                {
                    _staleTicks.Add(entry.Key);
                }
            }
            for (int i = 0; i < _staleTicks.Count; i++)
            {
                _remoteHashes.Remove(_staleTicks[i]);
            }
        }
    }
}
