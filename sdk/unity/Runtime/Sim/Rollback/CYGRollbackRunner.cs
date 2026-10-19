using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Deterministic rollback and state simulation runner.
    /// Orchestrates input prediction, snapshot history, and re-simulation on remote input receipt.
    /// Pure C# with zero runtime heap allocations during tick execution.
    /// </summary>
    public sealed class CYGRollbackRunner
    {
        public delegate void SimulationStepDelegate(
            int tick,
            CYGPlayerInput[] inputs,
            int[] inputPlayerIds,
            int inputCount,
            CYGEntityState[] entities,
            ref int entityCount,
            FP deltaTime);

        /// <summary>Optional custom simulation delegate. If null, built-in kinematic simulation runs.</summary>
        public event SimulationStepDelegate OnSimulateStep;

        /// <summary>Invoked whenever a rollback occurs: (rollbackFromTick, resimulatedToTick).</summary>
        public event Action<int, int> OnRollbackOccurred;

        private readonly CYGInputRingBuffer _inputRingBuffer;
        private readonly CYGStateRingBuffer _stateRingBuffer;

        private readonly CYGEntityState[] _entities;
        private int _entityCount;
        private int _nextEntityId = 1;

        // Preallocated zero-GC scratch arrays for gathering inputs per tick.
        // The id array is essential: registration order is not entity order.
        private readonly CYGPlayerInput[] _tickInputs;
        private readonly int[] _tickInputPlayerIds;

        private int _currentSimTick;
        private int _confirmedTick;
        private int _maxPredictionTicks = 10;
        private FP _deltaTime;
        private int _localPlayerId;

        private bool _isResimulating;
        private int _totalRollbacks;
        private int _totalResimulatedTicks;

        #region Properties

        /// <summary>Current speculative simulation tick index.</summary>
        public int CurrentSimTick => _currentSimTick;

        /// <summary>Last tick for which all player inputs have been authoritatively confirmed.</summary>
        public int ConfirmedTick => _confirmedTick;

        /// <summary>Maximum number of frames client can speculate ahead of confirmed tick.</summary>
        public int MaxPredictionTicks
        {
            get => _maxPredictionTicks;
            set => _maxPredictionTicks = Math.Max(1, value);
        }

        /// <summary>Fixed simulation time step per tick (e.g. 1/60s).</summary>
        public FP DeltaTime
        {
            get => _deltaTime;
            set => _deltaTime = value;
        }

        /// <summary>Local client's player ID.</summary>
        public int LocalPlayerId
        {
            get => _localPlayerId;
            set => _localPlayerId = value;
        }

        /// <summary>True during rollback resimulation loops (suppress audio/effects).</summary>
        public bool IsResimulating => _isResimulating;

        /// <summary>Total count of rollbacks executed since start.</summary>
        public int TotalRollbacks => _totalRollbacks;

        /// <summary>Total number of ticks resimulated across all rollbacks.</summary>
        public int TotalResimulatedTicks => _totalResimulatedTicks;

        /// <summary>The internal input ring buffer.</summary>
        public CYGInputRingBuffer InputRingBuffer => _inputRingBuffer;

        /// <summary>The internal world state snapshot buffer.</summary>
        public CYGStateRingBuffer StateRingBuffer => _stateRingBuffer;

        /// <summary>Active entity buffer.</summary>
        public CYGEntityState[] Entities => _entities;

        /// <summary>Count of active entities in the simulation world.</summary>
        public int EntityCount => _entityCount;

        #endregion

        public CYGRollbackRunner(
            int stateBufferSize = CYGStateRingBuffer.DefaultCapacity,
            int inputBufferSize = CYGInputRingBuffer.DefaultCapacity,
            int maxEntities = CYGStateRingBuffer.DefaultMaxEntities,
            int maxPlayers = CYGInputRingBuffer.DefaultMaxPlayers)
        {
            _inputRingBuffer = new CYGInputRingBuffer(inputBufferSize, maxPlayers);
            _stateRingBuffer = new CYGStateRingBuffer(stateBufferSize, maxEntities);

            _entities = new CYGEntityState[maxEntities];
            _tickInputs = new CYGPlayerInput[maxPlayers];
            _tickInputPlayerIds = new int[maxPlayers];

            // 60 Ticks per second: DeltaTime = 1 / 60
            _deltaTime = FP.One / FP.FromInt(60);

            _currentSimTick = 0;
            _confirmedTick = 0;
            _entityCount = 0;
            _localPlayerId = 0;

            // Save initial snapshot at tick 0
            _stateRingBuffer.SaveSnapshot(0, _entities, _entityCount, CYGChecksum.Compute(0, _entities, _entityCount));
        }

        /// <summary>
        /// Checks whether the simulation is within allowed prediction limits.
        /// If client is too far ahead of ConfirmedTick, simulation should pause until remote inputs catch up.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool CanStepSimulation()
        {
            return (_currentSimTick - _confirmedTick) <= _maxPredictionTicks;
        }

        /// <summary>
        /// Sets the local player's input for the current simulation tick.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void SetLocalInput(int tick, in CYGPlayerInput input)
        {
            _inputRingBuffer.SetInput(_localPlayerId, tick, input, isPredicted: false);
        }

        /// <summary>
        /// Advances simulation forward by one tick using gathered player inputs.
        /// Saves snapshot to state ring buffer with checksum.
        /// </summary>
        public void StepSimulation(int tick, CYGPlayerInput[] inputs)
        {
            // Compatibility overload for callers that supply inputs in
            // player-index order (id = index + 1). Network paths must use the
            // id-aware overload below.
            int inputCount = inputs != null ? inputs.Length : 0;
            if (inputCount > _tickInputPlayerIds.Length)
            {
                inputCount = _tickInputPlayerIds.Length;
            }
            for (int i = 0; i < inputCount; i++)
            {
                _tickInputPlayerIds[i] = i + 1;
            }
            SimulateAndSave(tick, inputs, _tickInputPlayerIds, inputCount);
        }

        /// <summary>
        /// Steps with an explicit player-id array so entity IDs are matched by
        /// actor id, not by the order inputs happened to arrive.
        /// </summary>
        public void StepSimulation(int tick, CYGPlayerInput[] inputs, int[] inputPlayerIds)
        {
            int inputCount = inputs != null ? inputs.Length : 0;
            if (inputCount > inputPlayerIds.Length)
            {
                inputCount = inputPlayerIds.Length;
            }
            if (inputCount > _tickInputPlayerIds.Length)
            {
                inputCount = _tickInputPlayerIds.Length;
            }
            SimulateAndSave(tick, inputs, inputPlayerIds, inputCount);
        }

        private void SimulateAndSave(
            int tick,
            CYGPlayerInput[] inputs,
            int[] inputPlayerIds,
            int inputCount)
        {
            if (OnSimulateStep != null)
            {
                OnSimulateStep(tick, inputs, inputPlayerIds, inputCount, _entities, ref _entityCount, _deltaTime);
            }
            else
            {
                DefaultSimulateStep(tick, inputs, inputPlayerIds, inputCount, _entities, ref _entityCount, _deltaTime);
            }

            _currentSimTick = tick + 1;
            uint checksum = CYGChecksum.Compute(_currentSimTick, _entities, _entityCount);
            _stateRingBuffer.SaveSnapshot(_currentSimTick, _entities, _entityCount, checksum);

            UpdateConfirmedTick();
        }

        /// <summary>
        /// Steps the simulation using inputs retrieved and predicted from the input ring buffer.
        /// </summary>
        public void Step()
        {
            _inputRingBuffer.GetAllInputs(
                _currentSimTick,
                _tickInputs,
                _tickInputPlayerIds,
                out int inputCount);
            SimulateAndSave(_currentSimTick, _tickInputs, _tickInputPlayerIds, inputCount);
        }

        /// <summary>
        /// Receives remote player input from the network.
        /// If input is for past tick T &lt; CurrentSimTick, triggers rollback to T, injects input,
        /// and resimulates ticks T through CurrentSimTick.
        /// </summary>
        /// <returns>True if a rollback and resimulation occurred, false otherwise.</returns>
        public bool OnRemoteInputReceived(int playerId, CYGPlayerInput input)
        {
            input = input.Clamped();
            int inputTick = input.Tick;

            // Reject malformed ownership and impossible ticks. A future tick
            // used to jump ConfirmedTick to 2,000,000,000 and freeze rollback;
            // too-old ticks would target snapshots that no longer exist.
            if (playerId <= 0 || playerId == _localPlayerId)
            {
                return false;
            }
            if (inputTick < 0
                || inputTick > _currentSimTick + _maxPredictionTicks
                || inputTick < _currentSimTick - _inputRingBuffer.Capacity)
            {
                return false;
            }

            // Check if input was already confirmed identically
            if (_inputRingBuffer.TryGetInput(playerId, inputTick, out CYGPlayerInput existing, out bool isPredicted))
            {
                if (!isPredicted && existing.ActionsEqual(in input))
                {
                    return false; // Identical confirmed input already applied
                }
            }

            // Store confirmed input
            _inputRingBuffer.SetInput(playerId, inputTick, input, isPredicted: false);

            // If input is for a past tick that was already simulated
            if (inputTick < _currentSimTick)
            {
                int rollbackTick = inputTick;

                // Clamp to oldest available snapshot in state ring buffer
                int earliestAvailable = _stateRingBuffer.EarliestTick;
                if (earliestAvailable >= 0 && rollbackTick < earliestAvailable)
                {
                    rollbackTick = earliestAvailable;
                }

                // 1. Restore snapshot at rollbackTick
                if (!_stateRingBuffer.TryRestoreSnapshot(rollbackTick, _entities, out _entityCount, out uint _))
                {
                    return false;
                }

                _isResimulating = true;
                _totalRollbacks++;
                _totalResimulatedTicks += (_currentSimTick - rollbackTick);

                // 2. Resimulate forward from rollbackTick to _currentSimTick
                for (int t = rollbackTick; t < _currentSimTick; t++)
                {
                    _inputRingBuffer.GetAllInputs(
                        t,
                        _tickInputs,
                        _tickInputPlayerIds,
                        out int inputCount);

                    if (OnSimulateStep != null)
                    {
                        OnSimulateStep(
                            t,
                            _tickInputs,
                            _tickInputPlayerIds,
                            inputCount,
                            _entities,
                            ref _entityCount,
                            _deltaTime);
                    }
                    else
                    {
                        DefaultSimulateStep(
                            t,
                            _tickInputs,
                            _tickInputPlayerIds,
                            inputCount,
                            _entities,
                            ref _entityCount,
                            _deltaTime);
                    }

                    uint checksum = CYGChecksum.Compute(t + 1, _entities, _entityCount);
                    _stateRingBuffer.SaveSnapshot(t + 1, _entities, _entityCount, checksum);
                }

                _isResimulating = false;

                UpdateConfirmedTick();

                OnRollbackOccurred?.Invoke(rollbackTick, _currentSimTick);
                return true;
            }

            // Input is for current or future tick, no rollback needed
            UpdateConfirmedTick();
            return false;
        }

        private void UpdateConfirmedTick()
        {
            int lowest = _inputRingBuffer.GetLowestConfirmedTick(_confirmedTick);
            if (lowest > _confirmedTick)
            {
                _confirmedTick = lowest;
            }
        }

        /// <summary>
        /// Default pure fixed-point kinematic 3D simulation.
        /// Applies player inputs, horizontal velocity, jump impulse, gravity, and ground clamping.
        /// </summary>
        private void DefaultSimulateStep(
            int tick,
            CYGPlayerInput[] inputs,
            int[] inputPlayerIds,
            int inputCount,
            CYGEntityState[] entities,
            ref int entityCount,
            FP dt)
        {
            FP moveSpeed = FP.FromInt(6);     // 6 units / sec
            FP jumpImpulse = FP.FromInt(8);   // 8 units / sec
            FP gravity = FP.FromInt(18);       // 18 units / sec^2

            for (int i = 0; i < entityCount; i++)
            {
                ref CYGEntityState entity = ref entities[i];
                if (!entity.IsActive) continue;

                // Match by actor id. Registration order can be reversed (the
                // second client's input often arrives first), so entityId - 1
                // would swap players.
                int inputIndex = -1;
                for (int j = 0; j < inputCount; j++)
                {
                    if (inputPlayerIds[j] == entity.EntityId)
                    {
                        inputIndex = j;
                        break;
                    }
                }

                if (inputIndex >= 0)
                {
                    CYGPlayerInput input = inputs[inputIndex].Clamped();

                    // Convert -1000..+1000 to fixed-point (-1.0 to +1.0)
                    FP inputX = FP.FromRaw(((long)input.MoveX << 32) / 1000);
                    FP inputZ = FP.FromRaw(((long)input.MoveY << 32) / 1000);

                    // Horizontal velocity
                    entity.Velocity.x = inputX * moveSpeed;
                    entity.Velocity.z = inputZ * moveSpeed;

                    // Ground check & Jump
                    bool isGrounded = entity.Position.y <= FP.Zero;
                    if (isGrounded && input.Jump)
                    {
                        entity.Velocity.y = jumpImpulse;
                    }
                }

                // Apply gravity
                entity.Velocity.y -= gravity * dt;

                // Integrate position
                entity.Position.x += entity.Velocity.x * dt;
                entity.Position.y += entity.Velocity.y * dt;
                entity.Position.z += entity.Velocity.z * dt;

                // Ground plane collision at Y = 0
                if (entity.Position.y < FP.Zero)
                {
                    entity.Position.y = FP.Zero;
                    if (entity.Velocity.y < FP.Zero)
                    {
                        entity.Velocity.y = FP.Zero;
                    }
                }

                // Update orientation if moving horizontally
                if (entity.Velocity.x != FP.Zero || entity.Velocity.z != FP.Zero)
                {
                    FP yaw = FP.Atan2(entity.Velocity.x, entity.Velocity.z) * FP.Rad2Deg;
                    entity.Rotation = FPQuaternion.Euler(FP.Zero, yaw, FP.Zero);
                }
            }
        }

        #region Entity Management

        /// <summary>
        /// Registers a new active entity into the simulation world.
        /// </summary>
        public int AddEntity(FPVector3 position, int health = 100)
        {
            if (_entityCount >= _entities.Length)
                throw new InvalidOperationException($"Cannot add entity: pool exhausted (Max: {_entities.Length})");

            int entityId = _nextEntityId++;
            int index = _entityCount++;
            _entities[index] = new CYGEntityState(entityId, position, health);
            return entityId;
        }

        /// <summary>
        /// Sets or inserts an entity with specific EntityId.
        /// </summary>
        public void SetEntityState(in CYGEntityState state)
        {
            for (int i = 0; i < _entityCount; i++)
            {
                if (_entities[i].EntityId == state.EntityId)
                {
                    _entities[i] = state;
                    return;
                }
            }

            if (_entityCount < _entities.Length)
            {
                _entities[_entityCount++] = state;
                if (state.EntityId >= _nextEntityId)
                    _nextEntityId = state.EntityId + 1;
            }
        }

        /// <summary>
        /// Looks up entity state by EntityId.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool TryGetEntityState(int entityId, out CYGEntityState state)
        {
            for (int i = 0; i < _entityCount; i++)
            {
                if (_entities[i].EntityId == entityId)
                {
                    state = _entities[i];
                    return true;
                }
            }

            state = default;
            return false;
        }

        /// <summary>
        /// Retrieves past state of an entity at a specific tick from the state ring buffer. Zero GC.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool TryGetEntityStateAtTick(int tick, int entityId, out CYGEntityState state)
        {
            if (_stateRingBuffer.TryGetSnapshotRef(tick, out var snapshot))
            {
                for (int i = 0; i < snapshot.EntityCount; i++)
                {
                    if (snapshot.Entities[i].EntityId == entityId)
                    {
                        state = snapshot.Entities[i];
                        return true;
                    }
                }
            }

            state = default;
            return false;
        }

        /// <summary>
        /// Retrieves previous tick state and current tick state for smooth visual interpolation in Unity views. Zero GC.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool TryGetEntityInterpolationStates(int entityId, out CYGEntityState previousState, out CYGEntityState currentState)
        {
            if (TryGetEntityState(entityId, out currentState))
            {
                if (TryGetEntityStateAtTick(_currentSimTick - 1, entityId, out previousState))
                {
                    return true;
                }
                previousState = currentState;
                return true;
            }

            previousState = default;
            currentState = default;
            return false;
        }

        /// <summary>
        /// Removes an entity by ID from the active simulation.
        /// </summary>
        public bool RemoveEntity(int entityId)
        {
            for (int i = 0; i < _entityCount; i++)
            {
                if (_entities[i].EntityId == entityId)
                {
                    // Swap with last active entity (O(1) removal, order not preserved)
                    int lastIndex = _entityCount - 1;
                    if (i < lastIndex)
                    {
                        _entities[i] = _entities[lastIndex];
                    }
                    _entities[lastIndex] = default;
                    _entityCount--;
                    return true;
                }
            }
            return false;
        }

        /// <summary>
        /// Resets the runner to initial clean state.
        /// </summary>
        public void Reset(int startTick = 0)
        {
            _currentSimTick = startTick;
            _confirmedTick = startTick;
            _inputRingBuffer.Clear();
            _stateRingBuffer.Clear();
            Array.Clear(_entities, 0, _entities.Length);
            _entityCount = 0;
            _nextEntityId = 1;
            _isResimulating = false;
            _totalRollbacks = 0;
            _totalResimulatedTicks = 0;

            _stateRingBuffer.SaveSnapshot(startTick, _entities, _entityCount, CYGChecksum.Compute(startTick, _entities, _entityCount));
        }

        #endregion
    }
}
