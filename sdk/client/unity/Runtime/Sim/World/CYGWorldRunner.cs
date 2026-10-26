using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Advances a <see cref="CYGWorld"/> one tick at a time and keeps a ring of
    /// recent snapshots so the simulation can be rolled back and re-simulated
    /// with corrected input.
    /// </summary>
    public sealed class CYGWorldRunner
    {
        private sealed class FrameSnapshot
        {
            public int Frame;
            public bool Valid;
            public CYGWorldSnapshot State;
        }

        private readonly CYGWorld _world;
        private readonly CYGDeterminismConfig _config;
        private readonly FrameSnapshot[] _ring;
        private readonly List<ICYGSnapshotParticipant> _participants = new List<ICYGSnapshotParticipant>();
        private ICYGSnapshotParticipant[] _sortedParticipants = Array.Empty<ICYGSnapshotParticipant>();
        private bool _participantsDirty;
        private int _next;
        private int _frame;
        private bool _hasInitialFrame;

        public CYGWorldRunner(CYGWorld world, int rollbackCapacity = CYGDeterminismConfig.DefaultRollbackCapacity)
            : this(world, CYGDeterminismConfig.Default, rollbackCapacity)
        {
        }

        public CYGWorldRunner(CYGWorld world, CYGDeterminismConfig config)
            : this(world, config, config != null ? config.RollbackCapacity : 0)
        {
        }

        private CYGWorldRunner(CYGWorld world, CYGDeterminismConfig config, int rollbackCapacity)
        {
            if (world == null)
            {
                throw new ArgumentNullException(nameof(world));
            }
            if (config == null)
            {
                throw new ArgumentNullException(nameof(config));
            }
            if (rollbackCapacity < 1)
            {
                throw new ArgumentOutOfRangeException(nameof(rollbackCapacity));
            }

            _world = world;
            _config = config;
            _world.SeedRng(config.Seed);
            _ring = new FrameSnapshot[rollbackCapacity];
            for (int i = 0; i < _ring.Length; i++)
            {
                _ring[i] = new FrameSnapshot();
            }
        }

        public CYGWorld World
        {
            get { return _world; }
        }

        public CYGDeterminismConfig Config
        {
            get { return _config; }
        }

        /// <summary>The frame number of the most recently completed tick.</summary>
        public int Frame
        {
            get { return _frame; }
        }

        /// <summary>Tick index used by the next <see cref="Tick(Action{CYGWorld}, bool)"/> call.</summary>
        public int CurrentTick
        {
            get { return _frame; }
        }

        public int RollbackCapacity
        {
            get { return _ring.Length; }
        }

        /// <summary>
        /// World checksum folded together with every registered participant in
        /// ascending participant-id order. With no participants this is exactly
        /// <see cref="CYGWorld.ComputeChecksum"/>.
        /// </summary>
        public uint Checksum
        {
            get
            {
                uint world = _world.ComputeChecksum();
                EnsureParticipantsSorted();
                if (_sortedParticipants.Length == 0)
                {
                    return world;
                }

                uint crc = 0xFFFFFFFFu;
                CYGChecksum.HashUInt(ref crc, world);
                CYGChecksum.HashInt(ref crc, _sortedParticipants.Length);
                for (int i = 0; i < _sortedParticipants.Length; i++)
                {
                    ICYGSnapshotParticipant participant = _sortedParticipants[i];
                    CYGChecksum.HashUInt(ref crc, participant.ParticipantId);
                    CYGChecksum.HashUInt(ref crc, participant.ComputeChecksum());
                }
                return crc ^ 0xFFFFFFFFu;
            }
        }

        /// <summary>
        /// Session fingerprint: the config contract plus the participant ids
        /// that affect the run. Exchange it before the first tick.
        /// </summary>
        public uint ComputeFingerprint()
        {
            EnsureParticipantsSorted();
            uint crc = 0xFFFFFFFFu;
            CYGChecksum.HashUInt(ref crc, _config.ComputeFingerprint());
            CYGChecksum.HashInt(ref crc, _sortedParticipants.Length);
            for (int i = 0; i < _sortedParticipants.Length; i++)
            {
                CYGChecksum.HashUInt(ref crc, _sortedParticipants[i].ParticipantId);
            }
            return crc ^ 0xFFFFFFFFu;
        }

        /// <summary>
        /// Adds deterministic state that must be captured, restored and hashed
        /// with the world. Registration must happen before the first tick so
        /// every snapshot in the ring covers the same participant set.
        /// </summary>
        public void RegisterParticipant(ICYGSnapshotParticipant participant)
        {
            if (participant == null)
            {
                throw new ArgumentNullException(nameof(participant));
            }
            if (_frame != 0 || _hasInitialFrame)
            {
                throw new InvalidOperationException(
                    "Participants must be registered before the first tick (current frame " + _frame + ").");
            }
            for (int i = 0; i < _participants.Count; i++)
            {
                if (_participants[i].ParticipantId == participant.ParticipantId)
                {
                    throw new InvalidOperationException(
                        "Participant id " + participant.ParticipantId + " is already registered.");
                }
            }

            int capacity = participant.SnapshotCapacity;
            if (capacity != 0 && capacity < _ring.Length)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(participant),
                    "Participant '" + participant.ParticipantName + "' snapshots " + capacity
                    + " ticks but the runner rollback capacity is " + _ring.Length + ".");
            }

            _participants.Add(participant);
            _participantsDirty = true;
        }

        /// <summary>Registered participants in ascending id order.</summary>
        public IReadOnlyList<ICYGSnapshotParticipant> Participants
        {
            get
            {
                EnsureParticipantsSorted();
                return _sortedParticipants;
            }
        }

        /// <summary>
        /// Runs one simulation step and records the resulting state. The
        /// snapshot is taken after the callback returns, so rolling back N
        /// frames restores the world exactly as it was at the end of that tick.
        /// Signals are cleared and the event buffer is opened before the
        /// callback runs.
        /// </summary>
        public void Tick(Action<CYGWorld> simulate, bool isResimulating = false)
        {
            if (simulate == null)
            {
                throw new ArgumentNullException(nameof(simulate));
            }

            BeginTick(isResimulating);
            simulate(_world);
            CompleteTick();
        }

        /// <summary>Variant that passes the simulation tick index to the callback.</summary>
        public void Tick(Action<CYGWorld, int> simulate, bool isResimulating = false)
        {
            if (simulate == null)
            {
                throw new ArgumentNullException(nameof(simulate));
            }

            int tick = BeginTick(isResimulating);
            simulate(_world, tick);
            CompleteTick();
        }

        /// <summary>Runs the registered systems for this tick inside the rollback loop.</summary>
        public void Tick(CYGSystemRunner systems, bool isResimulating = false)
        {
            Tick(systems, isResimulating, null, null, 0);
        }

        /// <summary>
        /// Runs the registered systems with this tick's player inputs. The
        /// direct overload avoids a per-tick closure in the rollback loop.
        /// </summary>
        public void Tick(
            CYGSystemRunner systems,
            bool isResimulating,
            CYGPlayerInput[] inputs,
            int[] inputPlayerIds,
            int inputCount)
        {
            if (systems == null)
            {
                throw new ArgumentNullException(nameof(systems));
            }

            int tick = BeginTick(isResimulating);
            systems.Tick(tick, isResimulating, inputs, inputPlayerIds, inputCount);
            CompleteTick();
        }

        /// <summary>Appends events that are ready for the view. Local events are ready immediately.</summary>
        public int DispatchEvents(List<CYGEvent> output, bool forceAll = false)
        {
            return _world.Events.DispatchReady(output, forceAll);
        }

        /// <summary>Confirms the server-authoritative tick, releasing verified events up to it.</summary>
        public void ConfirmUpTo(int tick)
        {
            _world.Events.ConfirmUpTo(tick);
        }

        /// <summary>
        /// Restores the world to the end of <paramref name="framesBack"/> ticks
        /// ago. Returns false when that frame is no longer in the ring.
        /// </summary>
        public bool Rollback(int framesBack)
        {
            if (framesBack < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(framesBack));
            }

            int target = _frame - framesBack;
            if (target < 0)
            {
                return false;
            }

            // Resimulation writes replacement snapshots for frames that were
            // already captured, so several slots can hold the same frame. The
            // newest write is the one that reflects the latest input, and it
            // is always the closest match walking backwards from _next.
            int found = -1;
            for (int step = 0; step < _ring.Length; step++)
            {
                int i = (_next - 1 - step + _ring.Length) % _ring.Length;
                FrameSnapshot slot = _ring[i];
                if (slot.Valid && slot.Frame == target)
                {
                    found = i;
                    break;
                }
            }

            if (found < 0)
            {
                return false;
            }

            _world.Restore(_ring[found].State);
            _frame = target;
            _world.Events.RollbackTo(target);
            _world.Signals.Clear();

            EnsureParticipantsSorted();
            for (int i = 0; i < _sortedParticipants.Length; i++)
            {
                _sortedParticipants[i].RestoreSnapshot(target);
                _sortedParticipants[i].DiscardSnapshotsAfter(target);
            }

            // Everything after the restore point (and duplicate snapshots of
            // the restore point itself) is superseded by the resimulation that
            // follows. Dropping them keeps later rollbacks from finding a
            // stale pre-correction state.
            for (int i = 0; i < _ring.Length; i++)
            {
                if (i != found && _ring[i].Valid && _ring[i].Frame >= target)
                {
                    _ring[i].Valid = false;
                }
            }

            return true;
        }

        public void ClearHistory()
        {
            for (int i = 0; i < _ring.Length; i++)
            {
                _ring[i].Valid = false;
            }

            EnsureParticipantsSorted();
            for (int i = 0; i < _sortedParticipants.Length; i++)
            {
                _sortedParticipants[i].DiscardSnapshotsAfter(-1);
            }

            _hasInitialFrame = false;
        }

        /// <summary>
        /// Captures the pre-tick state as frame 0 so the first simulation tick
        /// can be rolled back. Normally captured automatically before the first
        /// <see cref="Tick(Action{CYGWorld}, bool)"/>.
        /// </summary>
        public void CaptureInitialFrame()
        {
            if (_frame != 0)
            {
                throw new InvalidOperationException(
                    "The initial frame can only be captured before the first tick; current frame is " + _frame + ".");
            }

            SaveFrame(0);
            CaptureParticipants(0);
            _hasInitialFrame = true;
        }

        private int BeginTick(bool isResimulating)
        {
            int tick = _frame;
            if (tick == 0 && !_hasInitialFrame)
            {
                PreallocateRing();
                CaptureInitialFrame();
            }

            _world.Signals.Clear();
            _world.Events.BeginTick(tick, isResimulating);
            return tick;
        }

        /// <summary>
        /// Grows every ring snapshot to the world's working size on the first
        /// tick. Doing it here keeps later captures allocation-free instead of
        /// paying one buffer allocation per new tick until the ring fills.
        /// </summary>
        private void PreallocateRing()
        {
            for (int i = 0; i < _ring.Length; i++)
            {
                FrameSnapshot slot = _ring[i];
                if (slot.State == null)
                {
                    slot.State = _world.CreateSnapshot();
                }
                else
                {
                    _world.Capture(slot.State);
                }
                slot.Frame = -1;
                slot.Valid = false;
            }
        }

        private void CompleteTick()
        {
            _frame++;
            SaveFrame(_frame);
            CaptureParticipants(_frame);
        }

        private void CaptureParticipants(int tick)
        {
            EnsureParticipantsSorted();
            for (int i = 0; i < _sortedParticipants.Length; i++)
            {
                _sortedParticipants[i].CaptureSnapshot(tick);
            }
        }

        private void EnsureParticipantsSorted()
        {
            if (!_participantsDirty)
            {
                return;
            }

            _participantsDirty = false;
            if (_participants.Count == 0)
            {
                _sortedParticipants = Array.Empty<ICYGSnapshotParticipant>();
                return;
            }

            _sortedParticipants = _participants.ToArray();
            Array.Sort(_sortedParticipants, CompareParticipants);
        }

        private static int CompareParticipants(ICYGSnapshotParticipant left, ICYGSnapshotParticipant right)
        {
            return left.ParticipantId.CompareTo(right.ParticipantId);
        }

        private void SaveFrame(int frame)
        {
            FrameSnapshot slot = _ring[_next];
            if (slot.State == null)
            {
                slot.State = _world.CreateSnapshot();
            }
            else
            {
                _world.Capture(slot.State);
            }
            slot.Frame = frame;
            slot.Valid = true;
            _next = (_next + 1) % _ring.Length;
        }
    }
}
