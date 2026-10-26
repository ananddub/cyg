using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Simulation step used by <see cref="CYGWorldRollbackSession"/>. Inputs
    /// are matched by player id; entity order comes from the world.
    /// </summary>
    public delegate void CYGWorldSimulationStep(
        CYGWorld world,
        int tick,
        CYGPlayerInput[] inputs,
        int[] inputPlayerIds,
        int inputCount,
        FP deltaTime);

    /// <summary>Why an input packet was not applied to the rollback session.</summary>
    public enum CYGInputRejectionReason
    {
        None = 0,
        InvalidPlayer = 1,
        InvalidTick = 2,
        OlderThanRollbackWindow = 3,
        AheadOfPredictionWindow = 4,
        RollbackUnavailable = 5,
    }

    /// <summary>
    /// ECS-backed rollback and prediction session. Unlike the kinematic
    /// <see cref="CYGRollbackRunner"/>, state lives in a <see cref="CYGWorld"/>,
    /// so component data, events, globals, physics and navigation snapshots are
    /// all restored together.
    ///
    /// The default step runs <see cref="CYGCharacterMovementSystem"/> over
    /// entities that have a <see cref="CYGSimAgent"/>. Call
    /// <see cref="SetSystems"/> or subscribe <see cref="OnSimulateStep"/> before
    /// the first tick to replace it.
    /// </summary>
    public sealed class CYGWorldRollbackSession
    {
        private readonly CYGWorld _world;
        private readonly CYGWorldRunner _runner;
        private readonly CYGInputRingBuffer _inputs;
        private readonly CYGVerifiedFrameTracker _verified;
        private readonly CYGPlayerInput[] _tickInputs;
        private readonly int[] _tickInputPlayerIds;
        private readonly CYGPlayerInput[] _redundantInputs =
            new CYGPlayerInput[CYGInputRedundancy.MaxInputs];
        private readonly Action<CYGWorld, int> _eventStep;

        private CYGSystemRunner _systems;
        private int _localPlayerId = 1;
        private int _maxPredictionTicks;
        private FP _deltaTime;
        private int _confirmedTick;
        private bool _isResimulating;
        private int _pendingInputCount;
        private int _totalRollbacks;
        private int _totalResimulatedTicks;
        private int _rejectedRollbacks;
        private int _rejectedInputs;

        public event CYGWorldSimulationStep OnSimulateStep;

        public event Action<int, int> OnRollbackOccurred;

        /// <summary>
        /// Fired when a remote input cannot be applied (invalid player, outside
        /// the rollback/prediction window, or the rollback snapshot is gone).
        /// Diagnostics only; input application is still best-effort.
        /// </summary>
        public event Action<int, int, CYGInputRejectionReason> OnInputRejected;

        public CYGWorldRollbackSession(
            CYGWorld world,
            CYGDeterminismConfig config = null,
            int inputBufferSize = CYGInputRingBuffer.DefaultCapacity,
            int maxPlayers = 0,
            bool defaultCharacterMovement = true)
        {
            _world = world ?? throw new ArgumentNullException(nameof(world));
            Config = config ?? CYGDeterminismConfig.Default;
            _maxPredictionTicks = Math.Max(1, Config.MaxPredictionTicks);
            _deltaTime = Config.FixedDeltaTime;

            int players = maxPlayers > 0 ? maxPlayers : Config.MaxPlayers;
            _inputs = new CYGInputRingBuffer(inputBufferSize, players);
            _tickInputs = new CYGPlayerInput[players];
            _tickInputPlayerIds = new int[players];
            _eventStep = RunEventStep;
            _verified = new CYGVerifiedFrameTracker(Config.RollbackCapacity);
            _runner = new CYGWorldRunner(_world, Config);

            if (defaultCharacterMovement)
            {
                EnableDefaultCharacterMovement();
            }
        }

        public CYGWorld World
        {
            get { return _world; }
        }

        public CYGWorldRunner Runner
        {
            get { return _runner; }
        }

        public CYGDeterminismConfig Config { get; }

        public CYGSystemRunner Systems
        {
            get { return _systems; }
        }

        public CYGInputRingBuffer InputRingBuffer
        {
            get { return _inputs; }
        }

        public CYGVerifiedFrameTracker VerifiedFrames
        {
            get { return _verified; }
        }

        /// <summary>Frame number of the most recently completed tick.</summary>
        public int Frame
        {
            get { return _runner.Frame; }
        }

        public int CurrentTick
        {
            get { return _runner.CurrentTick; }
        }

        public int ConfirmedTick
        {
            get { return _confirmedTick; }
        }

        public int LocalPlayerId
        {
            get { return _localPlayerId; }
            set
            {
                if (value < 0)
                {
                    throw new ArgumentOutOfRangeException(nameof(value));
                }
                _localPlayerId = value;
            }
        }

        public int MaxPredictionTicks
        {
            get { return _maxPredictionTicks; }
            set { _maxPredictionTicks = Math.Max(1, value); }
        }

        public FP DeltaTime
        {
            get { return _deltaTime; }
            set { _deltaTime = value; }
        }

        public bool IsResimulating
        {
            get { return _isResimulating; }
        }

        public int TotalRollbacks
        {
            get { return _totalRollbacks; }
        }

        public int TotalResimulatedTicks
        {
            get { return _totalResimulatedTicks; }
        }

        /// <summary>
        /// Past inputs that were stored but could not be rolled back because
        /// the frame had already left the snapshot ring. A non-zero value
        /// means the local state was not corrected for those inputs.
        /// </summary>
        public int RejectedRollbacks
        {
            get { return _rejectedRollbacks; }
        }

        /// <summary>Remote inputs that were ignored or could not be applied.</summary>
        public int RejectedInputs
        {
            get { return _rejectedInputs; }
        }

        /// <summary>
        /// When true, a successful <see cref="Step"/> immediately confirms the
        /// simulated tick. Local-authority games use this so prediction never
        /// stalls waiting for a server that will never send a confirmation.
        /// </summary>
        public bool AutoConfirmLocalTicks { get; set; }

        /// <summary>How many frames the local simulation is ahead of confirmed input.</summary>
        public int PredictionLead
        {
            get { return _runner.Frame - _confirmedTick; }
        }

        /// <summary>World and participant state hash for the current frame.</summary>
        public uint Checksum
        {
            get { return _runner.Checksum; }
        }

        /// <summary>Config-plus-participant contract id exchanged before the first tick.</summary>
        public uint Fingerprint
        {
            get { return _runner.ComputeFingerprint(); }
        }

        /// <summary>
        /// Adds deterministic state outside the ECS world to the rollback ring
        /// and checksums. Must be called before the first tick.
        /// </summary>
        public void RegisterParticipant(ICYGSnapshotParticipant participant)
        {
            _runner.RegisterParticipant(participant);
        }

        /// <summary>
        /// Registers the default character movement system and its component.
        /// Must be called before the first tick.
        /// </summary>
        public void EnableDefaultCharacterMovement()
        {
            RequireNotStarted();
            if (!_world.TryGetStore(CYGSimAgent.TypeId, out _))
            {
                _world.Register<CYGSimAgent>();
            }

            _systems = new CYGSystemRunner(_world, Config);
            _systems.AddSystem(new CYGCharacterMovementSystem());
            _systems.Start();
        }

        /// <summary>Uses a custom scheduler instead of the default character system.</summary>
        public void SetSystems(CYGSystemRunner systems)
        {
            if (systems == null)
            {
                throw new ArgumentNullException(nameof(systems));
            }
            if (!ReferenceEquals(systems.World, _world))
            {
                throw new ArgumentException("System runner belongs to a different world.", nameof(systems));
            }
            RequireNotStarted();
            _systems = systems;
            _systems.Start();
        }

        public bool CanStepSimulation()
        {
            if (AutoConfirmLocalTicks)
            {
                return true;
            }
            return (_runner.Frame - _confirmedTick) <= _maxPredictionTicks;
        }

        /// <summary>Stores the local player's input for the given tick.</summary>
        public void SetLocalInput(int tick, in CYGPlayerInput input)
        {
            SetPlayerInput(_localPlayerId, tick, input, isPredicted: false);
        }

        /// <summary>
        /// Stores input for any local actor (split-screen, bot, or scripted
        /// player) before the tick is simulated. The input is marked confirmed
        /// so it participates in the normal prediction/confirmation flow.
        /// </summary>
        public void SetPlayerInput(int playerId, int tick, in CYGPlayerInput input, bool isPredicted = false)
        {
            if (playerId <= 0)
            {
                throw new ArgumentOutOfRangeException(nameof(playerId));
            }
            if (tick < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(tick));
            }

            CYGPlayerInput value = input.Clamped();
            value.Tick = tick;
            _inputs.SetInput(playerId, tick, value, isPredicted);
        }

        /// <summary>
        /// Advances one tick using local and predicted inputs. Returns false
        /// when the client is too far ahead of confirmed input.
        /// </summary>
        public bool Step()
        {
            if (!CanStepSimulation())
            {
                return false;
            }

            int tick = _runner.CurrentTick;
            _inputs.GetAllInputs(tick, _tickInputs, _tickInputPlayerIds, out int count);
            SimulateOne(tick, count, false);
            UpdateConfirmedTick();
            if (AutoConfirmLocalTicks && tick > _confirmedTick)
            {
                ConfirmUpTo(tick);
            }
            return true;
        }

        /// <summary>
        /// Applies remote input. Past ticks trigger an ECS rollback and
        /// resimulation; future ticks are stored for the next step.
        /// </summary>
        public bool OnRemoteInputReceived(int playerId, CYGPlayerInput input)
        {
            input = input.Clamped();
            int inputTick = input.Tick;
            int frame = _runner.Frame;

            if (playerId <= 0 || playerId == _localPlayerId)
            {
                Reject(playerId, inputTick, CYGInputRejectionReason.InvalidPlayer);
                return false;
            }
            if (inputTick < 0)
            {
                Reject(playerId, inputTick, CYGInputRejectionReason.InvalidTick);
                return false;
            }
            if (inputTick > frame + _maxPredictionTicks)
            {
                Reject(playerId, inputTick, CYGInputRejectionReason.AheadOfPredictionWindow);
                return false;
            }
            if (inputTick < frame - _runner.RollbackCapacity)
            {
                Reject(playerId, inputTick, CYGInputRejectionReason.OlderThanRollbackWindow);
                return false;
            }

            CYGPlayerInput existing;
            bool isPredicted;
            if (_inputs.TryGetInput(playerId, inputTick, out existing, out isPredicted)
                && !isPredicted
                && existing.ActionsEqual(in input))
            {
                return false;
            }

            _inputs.SetInput(playerId, inputTick, input, isPredicted: false);

            if (inputTick < frame)
            {
                int framesBack = frame - inputTick;
                if (!_runner.Rollback(framesBack) || _runner.Frame != inputTick)
                {
                    _rejectedRollbacks++;
                    Reject(playerId, inputTick, CYGInputRejectionReason.RollbackUnavailable);
                    UpdateConfirmedTick();
                    return false;
                }

                _isResimulating = true;
                _totalRollbacks++;
                _totalResimulatedTicks += framesBack;

                for (int tick = inputTick; tick < frame; tick++)
                {
                    _inputs.GetAllInputs(tick, _tickInputs, _tickInputPlayerIds, out int count);
                    SimulateOne(tick, count, true);
                }

                _isResimulating = false;
                UpdateConfirmedTick();
                OnRollbackOccurred?.Invoke(inputTick, frame);
                return true;
            }

            UpdateConfirmedTick();
            return false;
        }

        /// <summary>Decodes and applies a redundant input packet (opcode 19 payload).</summary>
        public int ApplyRedundantInputPacket(byte[] buffer, int offset, int length)
        {
            int playerId;
            int count;
            if (!CYGInputRedundancy.TryReadPacket(
                    buffer, offset, length, _redundantInputs, out playerId, out count))
            {
                return 0;
            }

            for (int i = 0; i < count; i++)
            {
                OnRemoteInputReceived(playerId, _redundantInputs[i]);
            }
            return count;
        }

        /// <summary>Compares a server checksum against the locally predicted frame.</summary>
        public bool ConfirmFrame(int tick, uint serverChecksum)
        {
            return _verified.MarkVerified(tick, serverChecksum);
        }

        public int DispatchEvents(List<CYGEvent> output, bool forceAll = false)
        {
            return _runner.DispatchEvents(output, forceAll);
        }

        public void ConfirmUpTo(int tick)
        {
            if (tick > _runner.Frame)
            {
                tick = _runner.Frame;
            }
            if (tick < 0)
            {
                return;
            }

            _runner.ConfirmUpTo(tick);
            if (tick > _confirmedTick)
            {
                _confirmedTick = tick;
            }
        }

        private void SimulateOne(int tick, int inputCount, bool isResimulating)
        {
            if (OnSimulateStep != null)
            {
                _pendingInputCount = inputCount;
                _runner.Tick(_eventStep, isResimulating);
            }
            else if (_systems != null)
            {
                _runner.Tick(_systems, isResimulating, _tickInputs, _tickInputPlayerIds, inputCount);
            }
            else
            {
                throw new InvalidOperationException(
                    "No simulation step configured. Enable default character movement, call SetSystems, or subscribe OnSimulateStep.");
            }

            _verified.MarkPredicted(_runner.Frame, _runner.Checksum);
        }

        private void RunEventStep(CYGWorld world, int frameTick)
        {
            OnSimulateStep(world, frameTick, _tickInputs, _tickInputPlayerIds, _pendingInputCount, _deltaTime);
        }

        private void UpdateConfirmedTick()
        {
            int lowest = _inputs.GetLowestConfirmedTick(_confirmedTick);
            if (lowest > _confirmedTick)
            {
                _confirmedTick = lowest;
            }
        }

        private void Reject(int playerId, int tick, CYGInputRejectionReason reason)
        {
            if (reason == CYGInputRejectionReason.None)
            {
                return;
            }

            _rejectedInputs++;
            Action<int, int, CYGInputRejectionReason> handler = OnInputRejected;
            if (handler != null)
            {
                handler(playerId, tick, reason);
            }
        }

        private void RequireNotStarted()
        {
            if (_runner.Frame != 0)
            {
                throw new InvalidOperationException(
                    "Simulation configuration cannot change after the first tick (current frame " + _runner.Frame + ").");
            }
        }
    }
}
