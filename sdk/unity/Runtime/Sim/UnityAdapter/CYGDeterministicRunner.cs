using System;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Main coordinator MonoBehaviour for deterministic quantum-style simulation.
    /// Manages the fixed tick timer (e.g. 60Hz), gathers inputs via CYGInputCollector,
    /// drives CYGRollbackRunner simulation steps, synchronizes visual CYGEntityViews,
    /// and processes inbound input tick packets (opcode 19) with rollback resimulation.
    /// </summary>
    [DisallowMultipleComponent]
    [AddComponentMenu("CookYourGame/Sim/CYG Deterministic Runner")]
    public class CYGDeterministicRunner : MonoBehaviour
    {
        public static CYGDeterministicRunner Instance { get; private set; }

        [Header("Simulation Timing")]
        [Tooltip("Simulation fixed frequency in Ticks Per Second (default 60Hz).")]
        [SerializeField] private int ticksPerSecond = 60;

        [Tooltip("Maximum frames the client can predict ahead of ConfirmedTick before pausing (e.g. 10 ticks).")]
        [SerializeField] private int maxPredictionTicks = 10;

        [Tooltip("Maximum simulation steps allowed in a single Unity frame to prevent spiral of death.")]
        [SerializeField] private int maxTicksPerFrame = 5;

        [Header("Components")]
        [Tooltip("Input collector component (auto-located if null).")]
        [SerializeField] private CYGInputCollector inputCollector;

        [Header("Entity Management")]
        [Tooltip("Optional prefab to instantiate for simulation entities if not manually assigned.")]
        [SerializeField] private GameObject defaultEntityPrefab;

        [Tooltip("Automatically spawns a local player entity into the simulation on Start.")]
        [SerializeField] private bool autoSpawnLocalEntity = true;

        [SerializeField] private Vector3 localSpawnPosition = new Vector3(0, 1, 0);

        [Header("Network")]
        [Tooltip("Automatically listen to CYGNetwork opcode 19 input ticks.")]
        [SerializeField] private bool listenToNetwork = true;

        [Header("Diagnostics")]
        [SerializeField] private bool showDebugStats = true;

        // Core deterministic runner
        private CYGRollbackRunner _runner;
        private float _tickInterval;
        private float _accumulator;

        // Visual views mapped by EntityId
        private readonly Dictionary<int, CYGEntityView> _views = new Dictionary<int, CYGEntityView>();
        private readonly List<int> _staleViewIds = new List<int>();

        private int _localEntityId = 0;

        #region Properties

        public CYGRollbackRunner RollbackRunner => _runner;
        public int CurrentSimTick => _runner != null ? _runner.CurrentSimTick : 0;
        public int ConfirmedTick => _runner != null ? _runner.ConfirmedTick : 0;
        public float TickInterval => _tickInterval;
        public int TicksPerSecond => ticksPerSecond;
        public int LocalEntityId => _localEntityId;
        public CYGInputCollector InputCollector => inputCollector;

        #endregion

        private void Awake()
        {
            if (Instance != null && Instance != this)
            {
                Destroy(gameObject);
                return;
            }
            Instance = this;

            _tickInterval = 1f / Mathf.Max(10, ticksPerSecond);

            if (inputCollector == null)
            {
                inputCollector = GetComponent<CYGInputCollector>();
                if (inputCollector == null)
                {
                    inputCollector = gameObject.AddComponent<CYGInputCollector>();
                }
            }

            // Initialize pure C# rollback runner
            _runner = new CYGRollbackRunner();
            _runner.MaxPredictionTicks = maxPredictionTicks;
            _runner.DeltaTime = FP.One / FP.FromInt(ticksPerSecond);
            _runner.LocalPlayerId = inputCollector.LocalActorId;

            // Hook rollback notification
            _runner.OnRollbackOccurred += HandleRollbackOccurred;

            // Discover all CYGEntityView components existing in the scene.
            // FindObjectsByType only exists in Unity 2022.2+; the package still
            // declares 2021.3 support.
#if UNITY_2022_2_OR_NEWER
            CYGEntityView[] existingViews = FindObjectsByType<CYGEntityView>(FindObjectsSortMode.None);
#else
            CYGEntityView[] existingViews = FindObjectsOfType<CYGEntityView>();
#endif
            foreach (CYGEntityView view in existingViews)
            {
                if (view.EntityId > 0 && !_views.ContainsKey(view.EntityId))
                {
                    _views[view.EntityId] = view;
                }
            }
        }

        private void OnEnable()
        {
            if (listenToNetwork)
            {
                CYGNetwork.OnInputTickReceived += HandleRemoteInputPacket;
                CYGNetwork.OnConnectedEvent += HandleConnected;
            }
        }

        private void OnDisable()
        {
            if (listenToNetwork)
            {
                CYGNetwork.OnInputTickReceived -= HandleRemoteInputPacket;
                CYGNetwork.OnConnectedEvent -= HandleConnected;
            }
        }

        private void Start()
        {
            if (autoSpawnLocalEntity)
            {
                EnsureLocalEntity();
            }
        }

        private void HandleConnected()
        {
            if (_runner != null && inputCollector != null)
            {
                EnsureLocalEntity();
            }
        }

        /// <summary>
        /// Creates (or renames) the local entity so its EntityId equals the
        /// local actor id. Using a hard-coded 1 let a remote actor 1 overwrite
        /// the local player, and made entity/input mapping depend on join order.
        /// </summary>
        private void EnsureLocalEntity()
        {
            if (_runner == null)
            {
                return;
            }

            int actor = inputCollector != null ? inputCollector.LocalActorId : 0;
            if (actor <= 0)
            {
                actor = _runner.LocalPlayerId > 0 ? _runner.LocalPlayerId : 1;
            }

            if (_localEntityId == 0)
            {
                _localEntityId = actor;
                _runner.LocalPlayerId = actor;
                if (!_runner.TryGetEntityState(actor, out _))
                {
                    FPVector3 spawnPos = new FPVector3(
                        FP.FromFloat(localSpawnPosition.x),
                        FP.FromFloat(localSpawnPosition.y),
                        FP.FromFloat(localSpawnPosition.z));
                    _runner.SetEntityState(new CYGEntityState(actor, spawnPos));
                }

                if (!_views.TryGetValue(actor, out CYGEntityView view) || view == null)
                {
                    view = CreateVisualForEntity(actor, localSpawnPosition);
                }

                if (_runner.TryGetEntityState(actor, out CYGEntityState initial))
                {
                    view.OnSimulationTick(in initial, _tickInterval);
                }
                return;
            }

            if (_localEntityId == actor)
            {
                _runner.LocalPlayerId = actor;
                return;
            }

            int oldId = _localEntityId;
            if (_runner.TryGetEntityState(oldId, out CYGEntityState state))
            {
                _runner.RemoveEntity(oldId);
                state.EntityId = actor;
                _runner.SetEntityState(state);
            }
            else
            {
                _runner.SetEntityState(new CYGEntityState(
                    actor,
                    new FPVector3(
                        FP.FromFloat(localSpawnPosition.x),
                        FP.FromFloat(localSpawnPosition.y),
                        FP.FromFloat(localSpawnPosition.z))));
            }

            _localEntityId = actor;
            _runner.LocalPlayerId = actor;

            if (_views.TryGetValue(oldId, out CYGEntityView existing) && existing != null)
            {
                _views.Remove(oldId);
                existing.Bind(actor);
                existing.gameObject.name = $"EntityView_{actor} (Local)";
                _views[actor] = existing;
            }
            else if (!_views.ContainsKey(actor))
            {
                CreateVisualForEntity(actor, localSpawnPosition);
            }
        }

        private void Update()
        {
            if (_runner == null) return;

            // Fixed simulation tick accumulator
            _accumulator += Time.deltaTime;

            int stepsProcessed = 0;
            while (_accumulator >= _tickInterval && stepsProcessed < maxTicksPerFrame)
            {
                // Check prediction ceiling: if client is too far ahead of server confirmed tick, wait!
                if (!_runner.CanStepSimulation())
                {
                    break;
                }

                ExecuteTick();
                _accumulator -= _tickInterval;
                stepsProcessed++;
            }

            // Sync visual views after simulation step(s)
            SyncAllEntityViews();
        }

        /// <summary>
        /// Executes a single deterministic simulation tick.
        /// Gathers inputs, advances physics/state, and computes state snapshot.
        /// </summary>
        private void ExecuteTick()
        {
            int currentTick = _runner.CurrentSimTick;

            // 1. Gather local player input and transmit to OpenOxide server
            inputCollector.LocalActorId = _runner.LocalPlayerId;
            inputCollector.PollAndDispatch(currentTick, _runner);

            // 2. Step the simulation forward (retrieves inputs for all players from input ring buffer)
            _runner.Step();
        }

        /// <summary>
        /// Synchronizes visual CYGEntityViews with current deterministic entity states.
        /// </summary>
        private void SyncAllEntityViews()
        {
            CYGEntityState[] entities = _runner.Entities;
            int count = _runner.EntityCount;

            for (int i = 0; i < count; i++)
            {
                ref CYGEntityState state = ref entities[i];
                if (!state.IsActive) continue;

                if (!_views.TryGetValue(state.EntityId, out CYGEntityView view) || view == null)
                {
                    Vector3 pos = new Vector3(
                        state.Position.x.AsFloat(),
                        state.Position.y.AsFloat(),
                        state.Position.z.AsFloat()
                    );
                    view = CreateVisualForEntity(state.EntityId, pos);
                }

                if (_runner.TryGetEntityInterpolationStates(state.EntityId, out CYGEntityState prevState, out CYGEntityState currState))
                {
                    view.OnSimulationTick(in prevState, in currState, _tickInterval);
                }
                else
                {
                    view.OnSimulationTick(in state, _tickInterval);
                }
            }
        }

        /// <summary>
        /// Called when CYGRollbackRunner rewinds and resimulates ticks due to late remote input.
        /// Instantly snaps views to the newly resimulated state.
        /// </summary>
        private void HandleRollbackOccurred(int fromTick, int toTick)
        {
            SyncAllEntityViews();
        }

        /// <summary>
        /// Ingests incoming opcode 19 (CYG_WIRE_INPUT_TICK) packet from CYGNetwork.
        /// Extracts client_tick, player_actor_id, and inputs (including redundant past inputs),
        /// and feeds them into CYGRollbackRunner.OnRemoteInputReceived().
        ///
        /// Framing:
        /// [0]      : opcode (19 = CYG_WIRE_INPUT_TICK)
        /// [1..8]   : room_id (8 bytes)
        /// [9..12]  : client_tick (u32 BE)
        /// [13..14] : player_actor_id (u16 BE)
        /// [15]     : input_len (u8)
        /// [16..]   : input_payload (N bytes)
        /// </summary>
        public void HandleRemoteInputPacket(ArraySegment<byte> packet)
        {
            if (packet.Array == null || packet.Count < 16) return;

            byte[] data = packet.Array;
            int offset = packet.Offset;
            int count = packet.Count;

            // Both the raw UDP path and the WebSocket ROOM_DATA_BYTES path
            // deliver the full frame with the opcode byte intact. Requiring it
            // avoids mistaking a room id whose first byte happens to be 19 for
            // an opcode.
            if (data[offset] != CYGNetwork.CYG_WIRE_INPUT_TICK)
            {
                return;
            }

            uint clientTick = ((uint)data[offset + 9] << 24)
                       | ((uint)data[offset + 10] << 16)
                       | ((uint)data[offset + 11] << 8)
                       | (uint)data[offset + 12];

            ushort actorId = (ushort)((data[offset + 13] << 8) | data[offset + 14]);
            byte inputLen = data[offset + 15];
            int payloadStart = offset + 16;

            // Ignore our own looped back inputs. Compare against both ids:
            // before the first connection the local entity may still use the
            // fallback id.
            if (actorId == 0
                || actorId == CYGNetwork.LocalActorNumber
                || actorId == _runner.LocalPlayerId
                || actorId == _localEntityId)
            {
                return;
            }

            if (inputLen != CYGPlayerInput.SizeInBytes
                && inputLen != CYGPlayerInput.SizeInBytes * 2
                && inputLen != CYGPlayerInput.SizeInBytes * 3)
            {
                return;
            }

            int availablePayload = count - (payloadStart - offset);
            if (availablePayload != inputLen)
            {
                return;
            }

            if (clientTick > int.MaxValue)
            {
                return;
            }

            // 1. Current tick input (offset 0 in payload, 9 bytes)
            CYGPlayerInput currentInput = CYGPlayerInput.ReadFrom(data, payloadStart);
            if (currentInput.Tick != (int)clientTick)
            {
                return;
            }

            // Validate the whole redundant envelope before applying anything:
            // a malformed predecessor must drop the packet atomically instead
            // of leaving the current tick half-applied.
            bool hasPrev1 = inputLen >= CYGPlayerInput.SizeInBytes * 2;
            bool hasPrev2 = inputLen >= CYGPlayerInput.SizeInBytes * 3;
            CYGPlayerInput prev1 = default;
            CYGPlayerInput prev2 = default;
            if (hasPrev1)
            {
                prev1 = CYGPlayerInput.ReadFrom(data, payloadStart + CYGPlayerInput.SizeInBytes);
                // Redundancy is only valid for the immediately preceding
                // ticks. Otherwise a peer could smuggle arbitrary/future
                // inputs under a valid current tick.
                if (prev1.Tick != currentInput.Tick - 1)
                {
                    return;
                }
            }
            if (hasPrev2)
            {
                prev2 = CYGPlayerInput.ReadFrom(data, payloadStart + CYGPlayerInput.SizeInBytes * 2);
                if (prev2.Tick != currentInput.Tick - 2)
                {
                    return;
                }
            }

            _runner.OnRemoteInputReceived(actorId, currentInput);
            if (hasPrev1)
            {
                _runner.OnRemoteInputReceived(actorId, prev1);
            }
            if (hasPrev2)
            {
                _runner.OnRemoteInputReceived(actorId, prev2);
            }

            // Ensure a visual view exists for this remote player entity
            if (!_views.ContainsKey(actorId))
            {
                // Register entity into simulation if absent
                if (!_runner.TryGetEntityState(actorId, out _))
                {
                    _runner.SetEntityState(new CYGEntityState(actorId, new FPVector3(FP.Zero, FP.One, FP.Zero)));
                }

                CreateVisualForEntity(actorId, Vector3.up);
            }
        }

        #region View Management

        private CYGEntityView CreateVisualForEntity(int id, Vector3 spawnPosition)
        {
            GameObject go;
            if (defaultEntityPrefab != null)
            {
                go = Instantiate(defaultEntityPrefab, spawnPosition, Quaternion.identity);
            }
            else
            {
                // Create a standard Unity primitive cube if no prefab assigned
                go = GameObject.CreatePrimitive(PrimitiveType.Cube);
                go.transform.position = spawnPosition;
                // Remove physics collider since simulation runs purely in fixed-point
                Collider col = go.GetComponent<Collider>();
                if (col != null) Destroy(col);
            }

            go.name = $"EntityView_{id}" + (id == _localEntityId ? " (Local)" : " (Remote)");

            CYGEntityView view = go.GetComponent<CYGEntityView>();
            if (view == null)
            {
                view = go.AddComponent<CYGEntityView>();
            }

            view.Bind(id);
            _views[id] = view;
            return view;
        }

        public CYGEntityView RegisterView(CYGEntityView view)
        {
            if (view == null) return null;
            _views[view.EntityId] = view;
            return view;
        }

        public void UnregisterView(int id)
        {
            _views.Remove(id);
        }

        public CYGEntityView GetView(int id)
        {
            _views.TryGetValue(id, out CYGEntityView view);
            return view;
        }

        #endregion

        #region Debug GUI

        private void OnGUI()
        {
            if (!showDebugStats || _runner == null) return;

            GUILayout.BeginArea(new Rect(10, 10, 320, 260), GUI.skin.box);
            GUILayout.Label($"<b>CookYourGame reference Sim</b> ({ticksPerSecond}Hz)");
            GUILayout.Label($"Current Sim Tick: {_runner.CurrentSimTick}");
            GUILayout.Label($"Confirmed Tick: {_runner.ConfirmedTick}");
            GUILayout.Label($"Prediction Ahead: {_runner.CurrentSimTick - _runner.ConfirmedTick} / {maxPredictionTicks} ticks");
            GUILayout.Label($"Active Entities: {_runner.EntityCount}");
            GUILayout.Label($"Total Rollbacks: {_runner.TotalRollbacks}");
            GUILayout.Label($"Resimulated Ticks: {_runner.TotalResimulatedTicks}");
            GUILayout.Label($"Local Player Actor: {_runner.LocalPlayerId}");
            GUILayout.Label($"UDP Transport: {(CYGNetwork.IsConnected ? "Connected" : "Disconnected")} ({CYGNetwork.Latency:F1}ms)");
            GUILayout.EndArea();
        }

        #endregion
    }
}
