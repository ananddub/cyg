using System;
using System.Text;
using CookYourGame;
using CookYourGame.Sim;
using UnityEngine;

namespace CookYourGame.SportsArena
{
    /// <summary>
    /// Optional two-client mode. Both instances run the deterministic Sports
    /// Arena simulation locally, but input is relayed through the CYG
    /// input hub and the simulation only advances when both human inputs for a
    /// tick have arrived. A small HELLO/READY/START handshake gives both peers
    /// the same tick-0 epoch, and periodic CRC hashes detect divergence.
    /// </summary>
    public sealed partial class SportsArenaBootstrap
    {
        [Header("Network (CYG backend)")]
        [Tooltip("Start with -network (or CYG_NETWORK=1) to join a two-client lockstep room.")]
        public bool networkMode;

        [Tooltip("CYG WebSocket host. Override with CYG_SERVER_HOST.")]
        public string networkServer = "127.0.0.1";

        [Tooltip("CYG game port (the HTTP/WS transport port). Override with CYG_SERVER_PORT.")]
        public int networkPort = 7777;

        [Tooltip("Input-hub room name. Override with CYG_ROOM.")]
        public string networkRoom = "cyg_sports";

        private const byte LockstepMagic = 0x4C;
        private const byte LockstepVersion = 1;
        private const byte LockstepHello = 1;
        private const byte LockstepReady = 2;
        private const byte LockstepStart = 3;
        private const byte LockstepHash = 4;
        private const int LockstepHeaderSize = 10;
        private const int MaxCatchUpTicks = 8;

        public bool NetworkMode => networkMode;

        /// <summary>Fighter slot driven by this window's keyboard (0 = P1, 1 = P2).</summary>
        public int LocalPlayerIndex { get; private set; }

        /// <summary>Fighter slot driven by the peer's relayed keyboard input.</summary>
        public int RemotePlayerIndex { get; private set; } = 1;

        public bool NetworkConnected => networkMode && CYGNetwork.IsConnected;
        public bool HasRemotePlayer { get; private set; }
        public bool HasRemoteInput => _lockstep != null && _lockstep.HasRemoteInput(Tick);

        public SportsArenaLockstepState LockstepState =>
            _lockstep != null ? _lockstep.State : SportsArenaLockstepState.Connecting;

        public bool HasDesync => _lockstep != null && _lockstep.HasDesync;
        public int DesyncTick => _lockstep != null ? _lockstep.DesyncTick : -1;
        public int VerifiedHashCount => _lockstep != null ? _lockstep.VerifiedHashCount : 0;
        public int LastStateHashTick => _lockstep != null ? _lockstep.LastLocalHashTick : -1;
        public uint LastStateHash => _lockstep != null ? _lockstep.LastLocalHash : 0;

        public string NetworkStatusText
        {
            get
            {
                if (!networkMode)
                {
                    return "OFFLINE";
                }
                if (!CYGNetwork.IsConnected || _lockstep == null)
                {
                    return "CONNECTING";
                }

                switch (_lockstep.State)
                {
                    case SportsArenaLockstepState.WaitingForPeer:
                        return "WAITING FOR PEER";
                    case SportsArenaLockstepState.Ready:
                        return "PEER READY";
                    case SportsArenaLockstepState.Running:
                        return "LOCKSTEP " + Tick + "  hashes " + _lockstep.VerifiedHashCount;
                    case SportsArenaLockstepState.Stalled:
                        return "WAITING FOR INPUT @ " + Tick;
                    case SportsArenaLockstepState.Paused:
                        return "PAUSED";
                    case SportsArenaLockstepState.Desync:
                        return "DESYNC @ " + _lockstep.DesyncTick;
                    default:
                        return "CONNECTING";
                }
            }
        }

        private SportsArenaLockstep _lockstep;
        private CYGNetworkManager _networkManager;
        private readonly byte[] _inputPacket = new byte[16 + CYGPlayerInput.SizeInBytes * 3];
        private readonly byte[] _inputRoomId = new byte[8];
        private readonly byte[] _lockstepPacket = new byte[32];
        private readonly byte[] _lockstepArgs = new byte[16];
        private string _inputRoomName;
        private CYGPlayerInput _previousInput1;
        private CYGPlayerInput _previousInput2;
        private bool _hasPreviousInput1;
        private bool _hasPreviousInput2;
        private int _remoteActor;
        private int _nextLocalInputTick;
        private int _startEpoch;
        private int _lastStartEpoch;
        private int _inputDelayOverride = SportsArenaLockstep.DefaultInputDelay;
        private float _lastHandshakeAt;
        private float _lastStepAt;
        private bool _networkStarted;
        private bool _rolesResolved;
        private bool _sawRemoteInputActivity;
        private bool _warnedRpcSend;

        private bool IsStartAuthority =>
            CYGNetwork.LocalActorNumber > 0
            && _remoteActor > 0
            && CYGNetwork.LocalActorNumber < _remoteActor;

        private void InitializeNetworkMode()
        {
            string[] args = Environment.GetCommandLineArgs();
            for (int i = 0; i < args.Length; i++)
            {
                string arg = args[i];
                if (arg == "-network" || arg == "-cygNetwork"
                    || string.Equals(arg, "CYG_NETWORK=1", StringComparison.OrdinalIgnoreCase))
                {
                    networkMode = true;
                }
            }

            if (Environment.GetEnvironmentVariable("CYG_NETWORK") == "1")
            {
                networkMode = true;
            }

            string host = Environment.GetEnvironmentVariable("CYG_SERVER_HOST");
            if (!string.IsNullOrEmpty(host))
            {
                networkServer = host;
            }

            string room = Environment.GetEnvironmentVariable("CYG_ROOM");
            if (!string.IsNullOrEmpty(room))
            {
                networkRoom = room;
            }

            string port = Environment.GetEnvironmentVariable("CYG_SERVER_PORT");
            int parsedPort;
            if (!string.IsNullOrEmpty(port) && int.TryParse(port, out parsedPort) && parsedPort > 0)
            {
                networkPort = parsedPort;
            }

            string inputDelay = Environment.GetEnvironmentVariable("CYG_INPUT_DELAY");
            int parsedDelay;
            if (!string.IsNullOrEmpty(inputDelay)
                && int.TryParse(inputDelay, out parsedDelay)
                && parsedDelay >= 0
                && parsedDelay <= 8)
            {
                _inputDelayOverride = parsedDelay;
            }

            if (networkMode)
            {
                playerTwoIsBot = false;
            }
        }

        private void StartNetworkMode()
        {
            if (!networkMode || _networkStarted)
            {
                return;
            }

            _networkStarted = true;
            playerTwoIsBot = false;
            _lockstep = new SportsArenaLockstep { InputDelay = _inputDelayOverride };
            if (Simulation != null)
            {
                Simulation.Players[0].IsBot = false;
                Simulation.Players[1].IsBot = false;
            }

            // Keep the network driver on its own persistent object so the
            // arena root can be destroyed normally without killing the socket.
            GameObject networkObject = new GameObject("CYG Sports Arena Network");
            UnityEngine.Object.DontDestroyOnLoad(networkObject);

            _networkManager = networkObject.AddComponent<CYGNetworkManager>();
            _networkManager.ServerUrl = "http://" + networkServer + ":" + networkPort;
            _networkManager.RoomName = networkRoom;
            _networkManager.AutoSpawnPlayer = false;

            CYGNetwork.OnInputTickReceived += HandleNetworkInputTick;
            CYGNetwork.OnRpcReceived += HandleLockstepRpc;
            CYGNetwork.OnJoinedRoom += HandleNetworkJoined;
            CYGNetwork.OnDisconnectedEvent += HandleNetworkDisconnected;

            // Force the WebSocket transport. The raw input hub is available on
            // the same port and the server accepts a direct CYG room upgrade,
            // so both windows can use the exact same session/actor derivation.
            CYGNetwork.TransportFactory = () => new CYGWebSocketTransport();
            CYGNetwork.ConnectToMatch(networkRoom, string.Empty, networkServer, networkPort);

            Debug.Log("[SportsArena] Connecting to CYG input hub at ws://"
                + networkServer + ":" + networkPort + "/" + networkRoom);
        }

        private void ShutdownNetworkMode()
        {
            if (!_networkStarted)
            {
                return;
            }

            CYGNetwork.OnInputTickReceived -= HandleNetworkInputTick;
            CYGNetwork.OnRpcReceived -= HandleLockstepRpc;
            CYGNetwork.OnJoinedRoom -= HandleNetworkJoined;
            CYGNetwork.OnDisconnectedEvent -= HandleNetworkDisconnected;
            if (_networkManager != null)
            {
                _networkManager.Shutdown();
            }
            CYGNetwork.TransportFactory = null;
            _lockstep = null;
            _networkStarted = false;
            _rolesResolved = false;
        }

        /// <summary>
        /// Called from FixedUpdate in network mode. The local clock advances only
        /// through TryStepNetworkTick, and only when the tick-keyed input gate is
        /// complete for both humans.
        /// </summary>
        private void StepNetworkFrame()
        {
            if (!networkMode || _lockstep == null)
            {
                return;
            }

            float now = Time.realtimeSinceStartup;
            if (!CYGNetwork.IsConnected)
            {
                _lockstep.SetState(SportsArenaLockstepState.Connecting);
                return;
            }

            if (CYGNetwork.LocalActorNumber <= 0)
            {
                return;
            }

            if (_remoteActor <= 0)
            {
                _lockstep.SetState(SportsArenaLockstepState.WaitingForPeer);
                if (now - _lastHandshakeAt >= 0.25f)
                {
                    _lastHandshakeAt = now;
                    SendLockstepHello();
                }
                return;
            }

            if (_lockstep.State == SportsArenaLockstepState.Desync
                || _lockstep.State == SportsArenaLockstepState.Paused)
            {
                return;
            }

            if (_lockstep.State != SportsArenaLockstepState.Running
                && _lockstep.State != SportsArenaLockstepState.Stalled)
            {
                if (now - _lastHandshakeAt >= 0.25f)
                {
                    _lastHandshakeAt = now;
                    SendLockstepReady();
                    if (IsStartAuthority)
                    {
                        TryStartNetworkMatch();
                    }
                }
                return;
            }

            PumpLocalInputs();
            int steps = 0;
            while (steps < MaxCatchUpTicks
                && _lockstep.State != SportsArenaLockstepState.Desync
                && TryStepNetworkTick())
            {
                steps++;
            }

            if (steps > 0)
            {
                _lastStepAt = now;
                if (_lockstep.State == SportsArenaLockstepState.Stalled)
                {
                    _lockstep.SetState(SportsArenaLockstepState.Running);
                }
            }
            else if (now - _lastStepAt > 0.5f
                && _lockstep.State == SportsArenaLockstepState.Running)
            {
                _lockstep.SetState(SportsArenaLockstepState.Stalled);
            }
        }

        /// <summary>
        /// Samples a short look-ahead window of local input. Sending future
        /// ticks gives the peer time to receive them before this client reaches
        /// that tick, which is what makes the strict gate usable without
        /// prediction/rollback.
        /// </summary>
        private void PumpLocalInputs()
        {
            int target = Tick + _lockstep.InputDelay;
            int guard = 0;
            while (_nextLocalInputTick <= target && guard++ < 64)
            {
                int tick = _nextLocalInputTick;
                CYGPlayerInput input = ReadInput(0, tick).Value;
                input.Tick = tick;
                _lockstep.StoreLocal(tick, input);
                SendLocalInput(input, tick);
                _nextLocalInputTick++;
            }
        }

        private bool TryStepNetworkTick()
        {
            int tick = Tick;
            if (!_lockstep.TryGetTickInputs(tick, out CYGPlayerInput local, out CYGPlayerInput remote))
            {
                return false;
            }

            for (int i = 0; i < _inputs.Length; i++)
            {
                if (i == LocalPlayerIndex)
                {
                    _inputs[i].Value = local;
                }
                else if (i == RemotePlayerIndex)
                {
                    _inputs[i].Value = remote;
                }
                else
                {
                    // P3-P6 stay local bots. They must not pick up the
                    // keyboard/arrow input of the two human slots.
                    _inputs[i].Value = new CYGPlayerInput(tick, 0, 0, 0);
                }
            }

            Simulation.TickStep(_inputs);
            Tick = Simulation.Tick;
            _lockstep.AdvanceTo(Tick);

            uint hash = Simulation.ComputeStateHash();
            if (!_lockstep.RecordLocalHash(tick, hash))
            {
                HandleDesync();
                return true;
            }

            if (tick % SportsArenaLockstep.HashInterval == 0)
            {
                SendLockstepHash(tick, hash);
            }
            if (tick % 300 == 0)
            {
                Debug.Log("[SportsArena] lockstep tick=" + tick
                    + " hash=0x" + hash.ToString("X8")
                    + " verified=" + _lockstep.VerifiedHashCount);
            }
            return true;
        }

        private void HandleDesync()
        {
            if (_lockstep == null || _lockstep.State == SportsArenaLockstepState.Desync)
            {
                return;
            }

            _lockstep.SetState(SportsArenaLockstepState.Desync);
            Debug.LogError("[SportsArena] DESYNC at tick " + _lockstep.DesyncTick
                + " local=0x" + _lockstep.DesyncLocalHash.ToString("X8")
                + " remote=0x" + _lockstep.DesyncRemoteHash.ToString("X8"));
        }

        /// <summary>
        /// Builds the same 16-byte input-hub header as CYGInputCollector and
        /// sends the exact tick plus two redundant predecessors.
        /// </summary>
        private void SendLocalInput(in CYGPlayerInput current, int tick)
        {
            if (!CYGNetwork.IsConnected || CYGNetwork.LocalActorNumber <= 0)
            {
                return;
            }

            string room = CYGNetwork.RoomName;
            if (string.IsNullOrEmpty(room))
            {
                return;
            }

            if (!string.Equals(room, _inputRoomName, StringComparison.Ordinal))
            {
                _inputRoomName = room;
                Array.Clear(_inputRoomId, 0, _inputRoomId.Length);
                byte[] roomBytes = Encoding.UTF8.GetBytes(room);
                Buffer.BlockCopy(roomBytes, 0, _inputRoomId, 0, Math.Min(roomBytes.Length, _inputRoomId.Length));
            }

            _inputPacket[0] = CYGNetwork.CYG_WIRE_INPUT_TICK;
            Buffer.BlockCopy(_inputRoomId, 0, _inputPacket, 1, _inputRoomId.Length);
            _inputPacket[9] = (byte)((tick >> 24) & 0xFF);
            _inputPacket[10] = (byte)((tick >> 16) & 0xFF);
            _inputPacket[11] = (byte)((tick >> 8) & 0xFF);
            _inputPacket[12] = (byte)(tick & 0xFF);
            int actor = CYGNetwork.LocalActorNumber;
            _inputPacket[13] = (byte)((actor >> 8) & 0xFF);
            _inputPacket[14] = (byte)(actor & 0xFF);

            CYGPlayerInput exact = current;
            exact.Tick = tick;
            int offset = 16;
            exact.WriteTo(_inputPacket, offset);
            offset += CYGPlayerInput.SizeInBytes;
            if (_hasPreviousInput1)
            {
                _previousInput1.WriteTo(_inputPacket, offset);
                offset += CYGPlayerInput.SizeInBytes;
                if (_hasPreviousInput2)
                {
                    _previousInput2.WriteTo(_inputPacket, offset);
                    offset += CYGPlayerInput.SizeInBytes;
                }
            }

            _inputPacket[15] = (byte)(offset - 16);
            CYGNetwork.SendRaw(CYGNetwork.CYG_WIRE_INPUT_TICK,
                new ArraySegment<byte>(_inputPacket, 0, offset));

            _previousInput2 = _previousInput1;
            _hasPreviousInput2 = _hasPreviousInput1;
            _previousInput1 = exact;
            _hasPreviousInput1 = true;
        }

        private void HandleNetworkJoined(string room)
        {
            int actor = CYGNetwork.LocalActorNumber;
            Debug.Log("[SportsArena] Joined CYG input-hub room '" + networkRoom + "' as actor " + actor);
            _lastHandshakeAt = 0f;
            SendLockstepHello();
        }

        private void HandleNetworkDisconnected(string reason)
        {
            HasRemotePlayer = false;
            _remoteActor = 0;
            _rolesResolved = false;
            _lockstep?.SetState(SportsArenaLockstepState.Paused);
            Debug.LogWarning("[SportsArena] CYG disconnected: " + reason);
        }

        private void HandleNetworkInputTick(ArraySegment<byte> payload)
        {
            byte[] data = payload.Array;
            if (data == null || payload.Count < 16 + CYGPlayerInput.SizeInBytes)
            {
                return;
            }

            int offset = payload.Offset;
            if (data[offset] != CYGNetwork.CYG_WIRE_INPUT_TICK)
            {
                return;
            }

            int actor = (data[offset + 13] << 8) | data[offset + 14];
            int inputLength = data[offset + 15];
            int localActor = CYGNetwork.LocalActorNumber;
            if (actor <= 0
                || actor == localActor
                || inputLength < CYGPlayerInput.SizeInBytes
                || inputLength % CYGPlayerInput.SizeInBytes != 0
                || payload.Count < 16 + inputLength)
            {
                return;
            }

            if (_remoteActor == 0)
            {
                _remoteActor = actor;
                HasRemotePlayer = true;
                ResolveNetworkRoles(actor);
            }
            if (actor != _remoteActor || _lockstep == null)
            {
                return;
            }

            int count = inputLength / CYGPlayerInput.SizeInBytes;
            for (int i = 0; i < count; i++)
            {
                CYGPlayerInput remote = CYGPlayerInput.ReadFrom(
                    data,
                    offset + 16 + i * CYGPlayerInput.SizeInBytes).Clamped();
                if (_lockstep.StoreRemote(remote.Tick, remote))
                {
                    if (!_sawRemoteInputActivity
                        && (remote.Buttons != 0 || remote.MoveX != 0 || remote.MoveY != 0))
                    {
                        _sawRemoteInputActivity = true;
                        Debug.Log("[SportsArena] Remote key input received for P"
                            + (RemotePlayerIndex + 1)
                            + " at tick " + remote.Tick
                            + " (move=" + remote.MoveX + "," + remote.MoveY
                            + " buttons=" + remote.Buttons + ")");
                    }
                }
            }
        }

        private void ResolveNetworkRoles(int remoteActor)
        {
            int localActor = CYGNetwork.LocalActorNumber;
            if (localActor <= 0 || remoteActor <= 0 || localActor == remoteActor)
            {
                return;
            }

            int localIndex = localActor < remoteActor ? 0 : 1;
            int remoteIndex = 1 - localIndex;
            if (_rolesResolved
                && LocalPlayerIndex == localIndex
                && RemotePlayerIndex == remoteIndex)
            {
                return;
            }

            // Both windows compare the same pair of derived actor ids, so they
            // always agree on who controls P1 and who controls P2.
            LocalPlayerIndex = localIndex;
            RemotePlayerIndex = remoteIndex;
            HasRemotePlayer = true;
            _rolesResolved = true;
            if (Simulation != null)
            {
                Simulation.Players[0].IsBot = false;
                Simulation.Players[1].IsBot = false;
            }

            Debug.Log("[SportsArena] Network roles: local actor " + localActor
                + " -> P" + (LocalPlayerIndex + 1)
                + ", remote actor " + remoteActor
                + " -> P" + (RemotePlayerIndex + 1));
        }

        private void TryStartNetworkMatch()
        {
            if (!IsStartAuthority
                || _lockstep == null
                || _lockstep.State == SportsArenaLockstepState.Running
                || _lockstep.State == SportsArenaLockstepState.Stalled
                || _lockstep.State == SportsArenaLockstepState.Desync
                || _lockstep.State == SportsArenaLockstepState.Paused)
            {
                return;
            }

            int epoch = _startEpoch + 1;
            _startEpoch = epoch;
            SendLockstepStart(epoch);
            BeginNetworkMatch(epoch, _lockstep.InputDelay);
        }

        private void BeginNetworkMatch(int epoch, int inputDelay)
        {
            if (_lockstep == null || Simulation == null)
            {
                return;
            }

            int localActor = CYGNetwork.LocalActorNumber;
            if (localActor <= 0 || _remoteActor <= 0)
            {
                return;
            }

            ResolveNetworkRoles(_remoteActor);
            _lastStartEpoch = epoch;
            _startEpoch = epoch;
            _lockstep.ResetForStart(
                localActor,
                _remoteActor,
                LocalPlayerIndex,
                RemotePlayerIndex,
                epoch,
                inputDelay);
            Simulation.ResetForMatch();

            Tick = 0;
            _nextLocalInputTick = 0;
            _previousInput1 = default;
            _previousInput2 = default;
            _hasPreviousInput1 = false;
            _hasPreviousInput2 = false;
            _sawRemoteInputActivity = false;
            _lastStepAt = Time.realtimeSinceStartup;
            _lastHandshakeAt = Time.realtimeSinceStartup;

            Debug.Log("[SportsArena] Lockstep START epoch=" + epoch
                + " local actor " + localActor + " -> P" + (LocalPlayerIndex + 1)
                + ", remote actor " + _remoteActor + " -> P" + (RemotePlayerIndex + 1)
                + ", input delay=" + inputDelay);
        }

        private void HandleLockstepRpc(ArraySegment<byte> payload)
        {
            byte[] data = payload.Array;
            if (_lockstep == null
                || data == null
                || payload.Count < LockstepHeaderSize + 2)
            {
                return;
            }

            int offset = payload.Offset;
            if (data[offset + 7] != LockstepMagic
                || data[offset + 8] != LockstepVersion)
            {
                return;
            }

            byte type = data[offset + 9];
            int actor = ReadU16(data, offset + 10);
            int localActor = CYGNetwork.LocalActorNumber;
            if (actor <= 0 || actor == localActor)
            {
                return;
            }

            if (_remoteActor == 0)
            {
                _remoteActor = actor;
                HasRemotePlayer = true;
                ResolveNetworkRoles(actor);
            }
            if (actor != _remoteActor)
            {
                return;
            }

            switch (type)
            {
                case LockstepHello:
                    if (_lockstep.State == SportsArenaLockstepState.Connecting
                        || _lockstep.State == SportsArenaLockstepState.WaitingForPeer)
                    {
                        _lockstep.SetState(SportsArenaLockstepState.Ready);
                    }
                    SendLockstepReady();
                    break;

                case LockstepReady:
                    if (_lockstep.State == SportsArenaLockstepState.Connecting
                        || _lockstep.State == SportsArenaLockstepState.WaitingForPeer)
                    {
                        _lockstep.SetState(SportsArenaLockstepState.Ready);
                    }
                    if (IsStartAuthority)
                    {
                        if (_lockstep.State == SportsArenaLockstepState.Running
                            || _lockstep.State == SportsArenaLockstepState.Stalled)
                        {
                            // Peer may have missed the first START. Re-send the
                            // same epoch; a peer that already started ignores it.
                            SendLockstepStart(_lastStartEpoch);
                        }
                        else
                        {
                            TryStartNetworkMatch();
                        }
                    }
                    break;

                case LockstepStart:
                    if (payload.Count < LockstepHeaderSize + 7
                        || _lockstep.State == SportsArenaLockstepState.Desync)
                    {
                        return;
                    }
                    int epoch = ReadI32(data, offset + 12);
                    int inputDelay = data[offset + 16];
                    if (epoch <= _lastStartEpoch)
                    {
                        return;
                    }
                    BeginNetworkMatch(epoch, inputDelay);
                    break;

                case LockstepHash:
                    if (payload.Count < LockstepHeaderSize + 10
                        || _lockstep.State == SportsArenaLockstepState.Desync)
                    {
                        return;
                    }
                    int hashTick = ReadI32(data, offset + 12);
                    uint remoteHash = ReadU32(data, offset + 16);
                    if (!_lockstep.RecordRemoteHash(hashTick, remoteHash))
                    {
                        HandleDesync();
                    }
                    break;
            }
        }

        private void SendLockstepHello()
        {
            WriteU16(_lockstepArgs, 0, (ushort)CYGNetwork.LocalActorNumber);
            SendLockstepMessage(LockstepHello, 2);
        }

        private void SendLockstepReady()
        {
            if (_remoteActor <= 0)
            {
                return;
            }
            WriteU16(_lockstepArgs, 0, (ushort)CYGNetwork.LocalActorNumber);
            SendLockstepMessage(LockstepReady, 2);
        }

        private void SendLockstepStart(int epoch)
        {
            if (epoch <= 0 || _remoteActor <= 0)
            {
                return;
            }
            WriteU16(_lockstepArgs, 0, (ushort)CYGNetwork.LocalActorNumber);
            WriteI32(_lockstepArgs, 2, epoch);
            _lockstepArgs[6] = (byte)(_lockstep != null ? _lockstep.InputDelay : _inputDelayOverride);
            SendLockstepMessage(LockstepStart, 7);
        }

        private void SendLockstepHash(int tick, uint hash)
        {
            WriteU16(_lockstepArgs, 0, (ushort)CYGNetwork.LocalActorNumber);
            WriteI32(_lockstepArgs, 2, tick);
            WriteU32(_lockstepArgs, 6, hash);
            SendLockstepMessage(LockstepHash, 10);
        }

        private void SendLockstepMessage(byte type, int argLength)
        {
            if (!CYGNetwork.IsConnected
                || CYGNetwork.LocalActorNumber <= 0
                || argLength < 0
                || argLength > _lockstepArgs.Length)
            {
                return;
            }

            // The server reads body[6] as the RPC target. The first six bytes
            // are a dummy view id/method pair because this sample has no
            // generated RPC router; target 1 means "other clients in the room".
            _lockstepPacket[0] = (byte)'C';
            _lockstepPacket[1] = (byte)'Y';
            _lockstepPacket[2] = (byte)'G';
            _lockstepPacket[3] = (byte)'1';
            _lockstepPacket[4] = (byte)'L';
            _lockstepPacket[5] = (byte)'K';
            _lockstepPacket[6] = 1;
            _lockstepPacket[7] = LockstepMagic;
            _lockstepPacket[8] = LockstepVersion;
            _lockstepPacket[9] = type;
            Buffer.BlockCopy(_lockstepArgs, 0, _lockstepPacket, LockstepHeaderSize, argLength);

            int total = LockstepHeaderSize + argLength;
            if (!CYGNetwork.SendRpcPayload(new ArraySegment<byte>(_lockstepPacket, 0, total))
                && !_warnedRpcSend)
            {
                _warnedRpcSend = true;
                Debug.LogWarning("[SportsArena] Could not send a lockstep control packet.");
            }
        }

        private static ushort ReadU16(byte[] data, int offset)
        {
            return (ushort)((data[offset] << 8) | data[offset + 1]);
        }

        private static int ReadI32(byte[] data, int offset)
        {
            return (data[offset] << 24)
                | (data[offset + 1] << 16)
                | (data[offset + 2] << 8)
                | data[offset + 3];
        }

        private static uint ReadU32(byte[] data, int offset)
        {
            return ((uint)data[offset] << 24)
                | ((uint)data[offset + 1] << 16)
                | ((uint)data[offset + 2] << 8)
                | data[offset + 3];
        }

        private static void WriteU16(byte[] data, int offset, ushort value)
        {
            data[offset] = (byte)(value >> 8);
            data[offset + 1] = (byte)value;
        }

        private static void WriteI32(byte[] data, int offset, int value)
        {
            data[offset] = (byte)((value >> 24) & 0xFF);
            data[offset + 1] = (byte)((value >> 16) & 0xFF);
            data[offset + 2] = (byte)((value >> 8) & 0xFF);
            data[offset + 3] = (byte)(value & 0xFF);
        }

        private static void WriteU32(byte[] data, int offset, uint value)
        {
            data[offset] = (byte)((value >> 24) & 0xFF);
            data[offset + 1] = (byte)((value >> 16) & 0xFF);
            data[offset + 2] = (byte)((value >> 8) & 0xFF);
            data[offset + 3] = (byte)(value & 0xFF);
        }
    }
}
