using System;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Which ECS system owns movement for player entities.
    /// </summary>
    public enum CYGSimulationMode
    {
        Kinematic = 0,
        Physics3D = 1,
        Physics2D = 2,
        Navigation = 3,
        Custom = 4,
    }

    /// <summary>Who confirms simulated ticks and releases prediction backpressure.</summary>
    public enum CYGSyncAuthority
    {
        /// <summary>Local authority while offline, server-confirmed while connected.</summary>
        Automatic = 0,

        /// <summary>This peer is authoritative; every simulated tick is confirmed immediately.</summary>
        LocalAuthority = 1,

        /// <summary>Only the server advances the confirmed tick.</summary>
        ServerConfirmed = 2,
    }

    /// <summary>
    /// Unity front-end for the ECS-backed deterministic simulation. All state
    /// lives in a <see cref="CYGWorld"/> driven by <see cref="CYGWorldRollbackSession"/>:
    /// scene authoring is baked into entities at startup, local input is
    /// predicted, opcode 19 packets trigger ECS rollback, and Unity views are
    /// driven from authoritative components every tick.
    /// </summary>
    [DisallowMultipleComponent]
    [AddComponentMenu("CookYourGame/Sim/CYG Deterministic Runner")]
    public class CYGDeterministicRunner : MonoBehaviour
    {
        public static CYGDeterministicRunner Instance { get; private set; }

        [Header("Simulation Timing")]
        [Tooltip("Simulation fixed frequency in Ticks Per Second (default 60Hz).")]
        [SerializeField] private int ticksPerSecond = 60;

        [Tooltip("Maximum frames the client can predict ahead of ConfirmedTick before pausing.")]
        [SerializeField] private int maxPredictionTicks = 10;

        [Tooltip("Maximum simulation steps allowed in a single Unity frame to prevent spiral of death.")]
        [SerializeField] private int maxTicksPerFrame = 5;

        [SerializeField] private int rollbackCapacity = 64;
        [SerializeField] private int maxPlayers = 8;
        [SerializeField] private ulong seed = 0UL;

        [Header("Simulation Mode")]
        [SerializeField] private CYGSimulationMode simulationMode = CYGSimulationMode.Kinematic;

        [Tooltip("Navmesh asset used by Navigation mode. Falls back to a scene CYGNavMeshAuthoring when empty.")]
        [SerializeField] private CYGNavMeshAsset navMeshAsset;

        [Header("Components")]
        [Tooltip("Input collector component (auto-located if null).")]
        [SerializeField] private CYGInputCollector inputCollector;

        [Header("Presentation")]
        [Tooltip("Scene-reference context injected into entity views.")]
        [SerializeField] private CYGViewContext viewContext;

        [Tooltip("Entity view updater (auto-located on this GameObject if null).")]
        [SerializeField] private CYGEntityViewUpdater entityViewUpdater;

        [Tooltip("Typed callback surface used for input polling and game lifecycle hooks.")]
        [SerializeField] private CYGCallbacks callbacks;

        [Tooltip("Fall back to the input collector when callbacks exist but submit no input for a tick.")]
        [SerializeField] private bool fallbackToInputCollector = true;

        [Header("Assets")]
        [Tooltip("Asset database copied into CYGAssetRegistry.Shared on startup for id-based simulation lookups.")]
        [SerializeField] private CYGAssetDatabase assetDatabase;

        [Tooltip("Register the asset database before systems and entities start.")]
        [SerializeField] private bool registerAssetDatabase = true;

        [Header("Static Mesh Colliders")]
        [Tooltip("Root whose CYGStaticMeshAuthoring children are baked. Empty bakes every authoring object in the scene.")]
        [SerializeField] private Transform staticMeshRoot;

        [Tooltip("Bake MeshCollider level geometry into deterministic static meshes on startup.")]
        [SerializeField] private bool bakeStaticMeshes = true;

        [Tooltip("Clear the global mesh registry before baking. Disable when several additive scenes contribute meshes.")]
        [SerializeField] private bool clearMeshRegistryOnStart;

        [Header("Countdown Timers")]
        [Tooltip("Tick every CYGCountdown component automatically with a built-in system.")]
        [SerializeField] private bool enableCountdownTimers = true;

        [Header("Scene Baking")]
        [Tooltip("Root whose CYGEntityAuthoring children are baked. Empty bakes every authoring object in the scene.")]
        [SerializeField] private Transform sceneRoot;

        [Tooltip("Bake scene authoring into deterministic ECS entities on Start.")]
        [SerializeField] private bool bakeScene = true;

        [Tooltip("Prefab instantiated for runtime-spawned entities (remote players).")]
        [SerializeField] private GameObject defaultEntityPrefab;

        [Tooltip("Automatically spawns a local player entity if no authoring object is marked local.")]
        [SerializeField] private bool autoSpawnLocalEntity = true;

        [SerializeField] private Vector3 localSpawnPosition = new Vector3(0, 1, 0);

        [Header("Network")]
        [Tooltip("Automatically listen to CYGNetwork opcode 19 input ticks.")]
        [SerializeField] private bool listenToNetwork = true;

        [Tooltip("Spawn a kinematic player entity when input arrives for an unknown remote player.")]
        [SerializeField] private bool spawnRemotePlayersOnInput = true;

        [SerializeField] private Vector3 remoteSpawnPosition = new Vector3(3, 1, 0);

        [Header("Authority")]
        [Tooltip("Who confirms ticks. Automatic confirms locally while offline; LocalAuthority never waits for a server.")]
        [SerializeField] private CYGSyncAuthority syncAuthority = CYGSyncAuthority.Automatic;

        [Tooltip("Allowed to start simulation automatically on Start. Disable when systems and entities are configured by a custom bootstrap.")]
        [SerializeField] private bool autoStartSimulation = true;

        [Header("Diagnostics")]
        [SerializeField] private bool showDebugStats = true;

        private CYGDeterminismConfig _config;
        private CYGWorld _world;
        private CYGWorldRollbackSession _session;
        private CYGSystemRunner _systems;
        private CYGViewRegistry _views;
        private CYGNavMesh _navMesh;

        private readonly List<CYGBakedEntity> _baked = new List<CYGBakedEntity>();
        private readonly List<CYGEntityAuthoring> _authoring = new List<CYGEntityAuthoring>();
        private readonly List<CYGEvent> _readyEvents = new List<CYGEvent>();
        private readonly List<CYGStaticMesh> _staticMeshes = new List<CYGStaticMesh>();
        private SimpleCollisionWorld _collisionWorld;

        private float _tickInterval;
        private float _accumulator;
        private CYGEntityRef _localEntity = CYGEntityRef.None;
        private int _localPlayerId;
        private int _localEntityPlayerId;
        private bool _snapViews;
        private bool _viewsDirty;
        private bool _initialized;
        private bool _started;

        #region Properties

        public CYGWorld World => _world;
        public CYGWorldRollbackSession Session => _session;
        public CYGWorldRunner Runner => _session != null ? _session.Runner : null;
        public CYGSystemRunner Systems => _systems;
        public CYGNavMesh NavMesh => _navMesh;
        public CYGViewRegistry Views => _views;
        public CYGAssetDatabase Assets => assetDatabase;
        public IReadOnlyList<CYGStaticMesh> StaticMeshes => _staticMeshes;
        public SimpleCollisionWorld CollisionWorld => _collisionWorld;
        public CYGSimulationMode SimulationMode
        {
            get => simulationMode;
            set => simulationMode = value;
        }

        public int CurrentSimTick => _session != null ? _session.CurrentTick : 0;
        public int ConfirmedTick => _session != null ? _session.ConfirmedTick : 0;
        public int LocalPlayerId => _localPlayerId;
        public CYGEntityRef LocalEntity => _localEntity;
        public int ActiveEntities => _world != null ? _world.EntityCount : 0;
        public int ViewCount => _views != null ? _views.ViewCount : 0;
        public uint Checksum => _session != null ? _session.Checksum : 0u;
        public float TickInterval => _tickInterval;
        public int TicksPerSecond => ticksPerSecond;
        public CYGInputCollector InputCollector => inputCollector;
        public CYGDeterminismConfig Config => _config;

        public CYGViewContext ViewContext
        {
            get
            {
                if (viewContext != null)
                {
                    return viewContext;
                }
                return entityViewUpdater != null ? entityViewUpdater.ViewContext : null;
            }
        }

        public CYGEntityViewUpdater EntityViews => entityViewUpdater;
        public CYGCallbacks Callbacks => callbacks;
        public bool IsLocalPlayer => _localPlayerId > 0;

        public CYGPlayerRef LocalPlayerRef
        {
            get
            {
                return _localPlayerId > 0
                    ? new CYGPlayerRef((ushort)_localPlayerId)
                    : CYGPlayerRef.None;
            }
        }
        public bool AutoSpawnLocalEntity
        {
            get => autoSpawnLocalEntity;
            set => autoSpawnLocalEntity = value;
        }

        public CYGSyncAuthority SyncAuthority
        {
            get => syncAuthority;
            set => syncAuthority = value;
        }

        public bool AutoStartSimulation
        {
            get => autoStartSimulation;
            set => autoStartSimulation = value;
        }

        public bool IsSimulationStarted => _started;

        /// <summary>
        /// Invoked on each simulation tick immediately prior to _session.Step().
        /// Local multi-agent systems and split-screen players populate their
        /// inputs here so all actors are stepped with synchronized inputs.
        /// This is an event: several systems can subscribe without clobbering
        /// each other.
        /// </summary>
        public event Action<int> OnBeforeSimulateTick;

        /// <summary>View-only events released by the rollback-safe event buffer.</summary>
        public event Action<CYGEvent> OnViewEvent;

        #endregion

        private void Awake()
        {
            if (Instance != null && Instance != this)
            {
                Destroy(gameObject);
                return;
            }
            Instance = this;

            Initialize();
        }

        private void OnDestroy()
        {
            if (_views != null)
            {
                UnhookViewEvents(_views);
                _views.Clear(false);
                _views = null;
            }
            if (Instance == this)
            {
                Instance = null;
            }
        }

        /// <summary>
        /// Builds the world, systems and rollback session. Called from Awake;
        /// public so a custom bootstrap can prepare assets first.
        /// </summary>
        public void Initialize()
        {
            if (_initialized)
            {
                return;
            }

            _tickInterval = 1f / Mathf.Max(10, ticksPerSecond);

            if (inputCollector == null)
            {
                inputCollector = GetComponent<CYGInputCollector>();
                if (inputCollector == null)
                {
                    inputCollector = gameObject.AddComponent<CYGInputCollector>();
                }
            }

            _config = new CYGDeterminismConfig(
                ticksPerSecond,
                Mathf.Max(1, maxPlayers),
                seed,
                CYGDeterminismConfig.DefaultChecksumInterval,
                Mathf.Max(1, rollbackCapacity),
                Mathf.Max(1, maxPredictionTicks));

            _world = new CYGWorld(
                _config.Seed,
                0UL,
                _config.MaxPlayers,
                _config.RollbackCapacity);

            if (!_world.TryGetStore(CYGSimAgent.TypeId, out _))
            {
                _world.Register<CYGSimAgent>();
            }

            if (!_world.TryGetStore(CYGViewBinding.TypeId, out _))
            {
                _world.Register<CYGViewBinding>();
            }

            if (callbacks == null)
            {
                callbacks = GetComponent<CYGCallbacks>();
            }
            if (callbacks == null)
            {
#if UNITY_2023_1_OR_NEWER
                callbacks = UnityEngine.Object.FindAnyObjectByType<CYGCallbacks>();
#else
                callbacks = UnityEngine.Object.FindObjectOfType<CYGCallbacks>();
#endif
            }

            // If callbacks are attached (custom game logic), default to Custom simulation mode
            if (callbacks != null && simulationMode == CYGSimulationMode.Kinematic)
            {
                simulationMode = CYGSimulationMode.Custom;
            }

            RegisterConfiguredAssets();
            BakeConfiguredMeshes();

            _systems = new CYGSystemRunner(_world, _config);
            CYGSystem system = CreateMovementSystem();
            if (system is CYGCharacterMovementSystem movement && _staticMeshes.Count > 0)
            {
                movement.CollisionWorld = _collisionWorld;
            }
            if (system is CYGPhysics3DSystem physics3D && _staticMeshes.Count > 0)
            {
                for (int i = 0; i < _staticMeshes.Count; i++)
                {
                    physics3D.AddStaticMesh(_staticMeshes[i]);
                }
            }
            if (system != null)
            {
                _systems.AddSystem(system);
            }
            if (enableCountdownTimers)
            {
                _systems.AddSystem(new CYGCountdownSystem());
            }

            _session = new CYGWorldRollbackSession(
                _world,
                _config,
                CYGInputRingBuffer.DefaultCapacity,
                _config.MaxPlayers,
                defaultCharacterMovement: false);
            _session.SetSystems(_systems);
            _session.OnRollbackOccurred += HandleRollbackOccurred;

            _localPlayerId = inputCollector.LocalActorId > 0 ? inputCollector.LocalActorId : 1;
            _session.LocalPlayerId = _localPlayerId;
            inputCollector.LocalActorId = _localPlayerId;
            if (entityViewUpdater == null)
            {
                entityViewUpdater = GetComponent<CYGEntityViewUpdater>();
            }

            if (entityViewUpdater != null)
            {
                _views = entityViewUpdater.CreateOrGetRegistry(this, defaultEntityPrefab);
            }
            else
            {
                _views = new CYGViewRegistry(transform, defaultEntityPrefab);
                _views.ViewContext = ViewContext;
            }

            HookViewEvents(_views);

            if (callbacks != null)
            {
                callbacks.NotifyInitSystems(_systems);
            }

            _initialized = true;
        }

        private CYGSystem CreateMovementSystem()
        {
            switch (simulationMode)
            {
                case CYGSimulationMode.Physics3D:
                    return new CYGPhysics3DSystem();
                case CYGSimulationMode.Physics2D:
                    return new CYGPhysics2DSystem();
                case CYGSimulationMode.Navigation:
                    _navMesh = ResolveNavMesh();
                    if (_navMesh == null)
                    {
                        Debug.LogError(
                            "[CYG] Navigation mode requires a CYGNavMeshAsset or a CYGNavMeshAuthoring in the scene. Falling back to kinematic movement.");
                        simulationMode = CYGSimulationMode.Kinematic;
                        return new CYGCharacterMovementSystem();
                    }
                    return new CYGNavigationSystem(_navMesh);
                case CYGSimulationMode.Custom:
                    return null;
                default:
                    return new CYGCharacterMovementSystem();
            }
        }

        private void RegisterConfiguredAssets()
        {
            if (!registerAssetDatabase || assetDatabase == null)
            {
                return;
            }

            assetDatabase.Populate(CYGAssetRegistry.Shared);
        }

        private void BakeConfiguredMeshes()
        {
            _staticMeshes.Clear();
            _collisionWorld = new SimpleCollisionWorld();

            if (!bakeStaticMeshes)
            {
                return;
            }

            if (clearMeshRegistryOnStart)
            {
                CYGMeshRegistry.Clear();
            }

            CYGStaticMeshBake.BakeAll(staticMeshRoot, _staticMeshes);
            CYGStaticMeshBake.Populate(_collisionWorld, _staticMeshes);
        }

        /// <summary>
        /// Adds every baked level mesh to a character-controller collision
        /// world. Use this from a custom bootstrap or a Custom-mode system.
        /// </summary>
        public void PopulateCollisionWorld(SimpleCollisionWorld world)
        {
            if (world == null)
            {
                return;
            }

            CYGStaticMeshBake.Populate(world, _staticMeshes);
        }

        private CYGNavMesh ResolveNavMesh()
        {            if (navMeshAsset != null)
            {
                return navMeshAsset.Build();
            }

            CYGNavMeshAuthoring authoring = FindNavMeshAuthoring();
            return authoring != null ? authoring.BuildNavMesh() : null;
        }

        private void Start()
        {
            if (autoStartSimulation)
            {
                StartSimulation();
            }
        }

        /// <summary>
        /// Bakes scene authoring and spawns the local player if needed.
        /// Called automatically on Start; idempotent so custom bootstraps and
        /// tests can drive it earlier.
        /// </summary>
        public void StartSimulation()
        {
            if (_world == null || _started)
            {
                return;
            }

            _started = true;
            if (_systems != null)
            {
                _systems.Start();
            }
            BakeSceneEntities();
            EnsureLocalEntity();
            if (callbacks != null)
            {
                callbacks.NotifyGameStart();
            }
        }

        /// <summary>
        /// Registers an entity spawned by a custom bootstrap as the local view
        /// target. Prevents the runner from spawning a duplicate local player.
        /// </summary>
        public void SetLocalEntity(CYGEntityRef entity)
        {
            if (_world != null && !entity.IsNone && !_world.Exists(entity))
            {
                throw new ArgumentException("Local entity does not exist in this world.", nameof(entity));
            }

            _localEntity = entity;
            _localEntityPlayerId = _localPlayerId;
            _viewsDirty = true;
        }

        /// <summary>
        /// Writes a deterministic prefab binding into the ECS world. Views are
        /// created from this data on every client, including after a snapshot
        /// restore. The prefab id is resolved through the view updater's
        /// <see cref="CYGEntityViewPrefabRegistry"/>.
        /// </summary>
        public void SetEntityView(
            CYGEntityRef entity,
            CYGAssetId prefab,
            byte createBehaviour = CYGViewBinding.BindVerified,
            byte flags = CYGViewBinding.FlagPool | CYGViewBinding.FlagVisible)
        {
            if (_world == null)
            {
                throw new InvalidOperationException("Initialize the runner before assigning entity views.");
            }

            if (!_world.Exists(entity))
            {
                throw new ArgumentException("Entity does not exist in this world.", nameof(entity));
            }

            EnsureViewStore();
            _world.Set(entity, CYGViewBinding.Create(prefab, createBehaviour, flags));
            _viewsDirty = true;
        }

        /// <summary>Resolves a prefab key through the view registry, then stores its stable id.</summary>
        public bool SetEntityView(CYGEntityRef entity, string prefabKey)
        {
            if (_views == null || _views.PrefabRegistry == null || string.IsNullOrEmpty(prefabKey))
            {
                return false;
            }

            CYGEntityViewPrefabAsset asset = _views.PrefabRegistry.ResolveAsset(prefabKey);
            if (asset == null)
            {
                return false;
            }

            SetEntityView(entity, new CYGAssetId(asset.AssetId));
            return true;
        }

        private void EnsureViewStore()
        {
            if (!_world.TryGetStore(CYGViewBinding.TypeId, out _))
            {
                _world.Register<CYGViewBinding>();
            }
        }

        /// <summary>
        /// Adopts an entity view updater created outside the runner (e.g. a
        /// component on a child GameObject). Existing prefab bindings are
        /// preserved and old views are rebuilt with the new configuration.
        /// </summary>
        public void AttachViewUpdater(CYGEntityViewUpdater updater)
        {
            if (updater == null || ReferenceEquals(entityViewUpdater, updater))
            {
                return;
            }

            entityViewUpdater = updater;
            if (!_initialized)
            {
                return;
            }

            CYGViewRegistry previous = _views;
            CYGViewRegistry next = updater.CreateOrGetRegistry(this, defaultEntityPrefab);
            if (!ReferenceEquals(previous, next))
            {
                if (previous != null)
                {
                    UnhookViewEvents(previous);
                    previous.CopyBindingsTo(next);
                    previous.Clear(true);
                }
                _views = next;
                HookViewEvents(_views);
                _viewsDirty = true;
            }

            if (viewContext == null)
            {
                viewContext = updater.ViewContext;
            }
        }

        public void AttachCallbacks(CYGCallbacks value)
        {
            if (value != null && !ReferenceEquals(callbacks, value))
            {
                callbacks = value;
                if (_systems != null)
                {
                    callbacks.NotifyInitSystems(_systems);
                }
                if (_started)
                {
                    callbacks.NotifyGameStart();
                }
            }
        }

        private void HookViewEvents(CYGViewRegistry views)
        {
            if (views == null)
            {
                return;
            }
            views.ViewSpawned += HandleViewSpawned;
            views.ViewDespawned += HandleViewDespawned;
            views.ViewBound += HandleViewBound;
        }

        private void UnhookViewEvents(CYGViewRegistry views)
        {
            if (views == null)
            {
                return;
            }
            views.ViewSpawned -= HandleViewSpawned;
            views.ViewDespawned -= HandleViewDespawned;
            views.ViewBound -= HandleViewBound;
        }

        private void HandleViewSpawned(CYGEntityView view) => callbacks?.NotifyViewSpawned(view);
        private void HandleViewDespawned(CYGEntityView view) => callbacks?.NotifyViewDespawned(view);
        private void HandleViewBound(CYGEntityView view) => callbacks?.NotifyViewBound(view);

        /// <summary>
        /// Advances exactly one tick outside the Unity update loop. Returns
        /// false at the prediction ceiling. Used by tests and bot harnesses.
        /// </summary>
        public bool StepOnce()
        {
            if (_session == null || !_started || !_session.CanStepSimulation())
            {
                return false;
            }

            ExecuteTick();
            _viewsDirty = true;
            return true;
        }

        /// <summary>Applies the latest ECS state to all views.</summary>
        public void SyncViews(bool snap = false)
        {
            if (_session == null || _views == null)
            {
                _snapViews = false;
                _viewsDirty = false;
                return;
            }

            bool predicted = _session.CurrentTick > _session.ConfirmedTick;
            _views.Sync(_world, _tickInterval, snap, _localPlayerId, predicted);
            _snapViews = false;
            _viewsDirty = false;
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

        private void Update()
        {
            if (_session == null) return;

            _accumulator += Time.deltaTime;

            int steps = 0;
            while (_accumulator >= _tickInterval && steps < Mathf.Max(1, maxTicksPerFrame))
            {
                if (!_session.CanStepSimulation())
                {
                    break;
                }

                ExecuteTick();
                _accumulator -= _tickInterval;
                steps++;
            }

            if (steps > 0 || _snapViews || _viewsDirty)
            {
                SyncViews(_snapViews);
            }

            if (steps > 0 && callbacks != null)
            {
                callbacks.NotifySimulationUpdate();
            }
            if (callbacks != null)
            {
                callbacks.NotifyViewUpdate();
            }

            if (_world != null)
            {
                _readyEvents.Clear();
                _world.Events.DispatchReady(_readyEvents);
                Action<CYGEvent> viewHandler = OnViewEvent;
                for (int i = 0; i < _readyEvents.Count; i++)
                {
                    CYGEvent evt = _readyEvents[i];
                    CYGEvents.Dispatch(evt);
                    if (viewHandler != null)
                    {
                        viewHandler(evt);
                    }
                }
            }
        }

        private void LateUpdate()
        {
            if (callbacks != null)
            {
                callbacks.NotifyLateViewUpdate();
            }
        }

        private void ExecuteTick()
        {
            int tick = _session.CurrentTick;

            bool callbacksProvidedInput = false;
            if (callbacks != null)
            {
                callbacks.PollForTick(tick, _localPlayerId);
                if (callbacks.HasInputThisTick)
                {
                    _session.SetPlayerInput(
                        _localPlayerId,
                        tick,
                        callbacks.PolledInput,
                        isPredicted: false);
                    callbacksProvidedInput = true;

                    if (inputCollector != null)
                    {
                        inputCollector.DispatchInput(callbacks.PolledInput, tick);
                    }
                }
            }

            if (!callbacksProvidedInput && (callbacks == null || fallbackToInputCollector))
            {
                inputCollector.LocalActorId = _localPlayerId;
                inputCollector.PollAndDispatch(tick, _session);
            }

            _session.AutoConfirmLocalTicks = ShouldAutoConfirmLocalTicks();

            Action<int> beforeStep = OnBeforeSimulateTick;
            if (beforeStep != null)
            {
                beforeStep(tick);
            }

            _session.Step();
        }

        private bool ShouldAutoConfirmLocalTicks()
        {
            switch (syncAuthority)
            {
                case CYGSyncAuthority.LocalAuthority:
                    return true;
                case CYGSyncAuthority.ServerConfirmed:
                    return false;
                default:
                    return !listenToNetwork || !CYGNetwork.IsConnected;
            }
        }

        #region Scene Baking

        private void BakeSceneEntities()
        {
            if (!bakeScene)
            {
                return;
            }

            _authoring.Clear();
            if (sceneRoot != null)
            {
                sceneRoot.GetComponentsInChildren(true, _authoring);
            }
            else
            {
#if UNITY_2022_2_OR_NEWER
                CYGEntityAuthoring[] found = FindObjectsByType<CYGEntityAuthoring>(FindObjectsSortMode.None);
#else
                CYGEntityAuthoring[] found = FindObjectsOfType<CYGEntityAuthoring>();
#endif
                _authoring.AddRange(found);
            }

            if (_authoring.Count == 0)
            {
                return;
            }

            CYGSceneBaker.Bake(_world, _authoring, _localPlayerId, _baked);

            for (int i = 0; i < _baked.Count; i++)
            {
                CYGBakedEntity baked = _baked[i];
                if (!string.IsNullOrEmpty(baked.Authoring.ViewPrefabKey))
                {
                    if (_views != null)
                    {
                        _views.SetPrefabKey(baked.Entity, baked.Authoring.ViewPrefabKey);
                    }

                    if (_views != null && _views.PrefabRegistry != null)
                    {
                        CYGEntityViewPrefabAsset asset =
                            _views.PrefabRegistry.ResolveAsset(baked.Authoring.ViewPrefabKey);
                        if (asset != null && asset.AssetId != 0)
                        {
                            EnsureViewStore();
                            _world.Set(
                                baked.Entity,
                                CYGViewBinding.Create(new CYGAssetId(asset.AssetId)));
                        }
                    }
                }
                if (baked.Authoring.ViewPrefab != null && _views != null)
                {
                    _views.SetPrefab(baked.Entity, baked.Authoring.ViewPrefab);
                }
                if (baked.Authoring.IsLocalPlayer && _localEntity.IsNone)
                {
                    _localEntity = baked.Entity;
                    _localEntityPlayerId = _localPlayerId;
                }
            }
        }

        private void EnsureLocalEntity()
        {
            if (_localEntity.IsNone)
            {
                _localPlayerId = inputCollector.LocalActorId > 0 ? inputCollector.LocalActorId : _localPlayerId;
                _session.LocalPlayerId = _localPlayerId;
                if (TryFindEntityByPlayerId(_localPlayerId, out CYGEntityRef existing))
                {
                    _localEntity = existing;
                    _localEntityPlayerId = _localPlayerId;
                    return;
                }
                if (!autoSpawnLocalEntity)
                {
                    return;
                }
                _localEntity = SpawnPlayer(_localPlayerId, localSpawnPosition);
                _localEntityPlayerId = _localPlayerId;
                return;
            }

            _localPlayerId = inputCollector.LocalActorId > 0 ? inputCollector.LocalActorId : _localPlayerId;
            _session.LocalPlayerId = _localPlayerId;
            if (_localEntityPlayerId != _localPlayerId)
            {
                ApplyLocalPlayerId(_localPlayerId);
            }
        }

        private void HandleConnected()
        {
            if (_session == null)
            {
                return;
            }

            int actor = inputCollector != null ? inputCollector.LocalActorId : 0;
            if (actor <= 0 || actor == _localPlayerId)
            {
                return;
            }

            _localPlayerId = actor;
            _session.LocalPlayerId = actor;
            inputCollector.LocalActorId = actor;

            if (_localEntity.IsNone)
            {
                if (autoSpawnLocalEntity)
                {
                    _localEntity = SpawnPlayer(actor, localSpawnPosition);
                    _localEntityPlayerId = actor;
                }
            }
            else if (_localEntityPlayerId != actor)
            {
                ApplyLocalPlayerId(actor);
            }

            _viewsDirty = true;
        }

        private void ApplyLocalPlayerId(int playerId)
        {
            if (_world == null || _localEntity.IsNone || !_world.Exists(_localEntity))
            {
                return;
            }

            CYGSimAgent agent;
            if (_world.TryGet(_localEntity, out agent))
            {
                agent.PlayerId = playerId;
                _world.Set(_localEntity, agent);
            }

            CYGPhysicsBody3D body3D;
            if (_world.TryGet(_localEntity, out body3D))
            {
                body3D.PlayerId = playerId;
                _world.Set(_localEntity, body3D);
            }

            CYGPhysicsBody2D body2D;
            if (_world.TryGet(_localEntity, out body2D))
            {
                body2D.PlayerId = playerId;
                _world.Set(_localEntity, body2D);
            }

            CYGNavAgent nav;
            if (_world.TryGet(_localEntity, out nav))
            {
                nav.PlayerId = playerId;
                _world.Set(_localEntity, nav);
            }

            _localEntityPlayerId = playerId;
            _viewsDirty = true;
        }

        /// <summary>
        /// Spawns a player entity at runtime. Every peer must spawn the same
        /// player at the same tick for checksums to agree; normally scene
        /// authoring or a server spawn message should drive this.
        /// </summary>
        public CYGEntityRef SpawnPlayer(int playerId, Vector3 position, int health = 100)
        {
            if (_world == null)
            {
                throw new InvalidOperationException("Runner is not initialized.");
            }

            FPVector3 fixedPosition = new FPVector3(
                FP.FromFloat(position.x),
                FP.FromFloat(position.y),
                FP.FromFloat(position.z));

            CYGEntityRef entity = _world.CreateEntity();
            _world.Set(entity, CYGSimAgent.Create(playerId, fixedPosition, health));

            switch (simulationMode)
            {
                case CYGSimulationMode.Physics3D:
                    _world.Set(entity, CYGPhysicsBody3D.DynamicCapsule(
                        playerId,
                        fixedPosition,
                        FP.FromFraction(1, 2),
                        FP.FromInt(2),
                        FP.One));
                    break;
                case CYGSimulationMode.Physics2D:
                    _world.Set(entity, CYGPhysicsBody2D.DynamicCircle(
                        playerId,
                        new FPVector2(fixedPosition.x, fixedPosition.y),
                        FP.FromFraction(1, 2),
                        FP.One));
                    break;
                case CYGSimulationMode.Navigation:
                    _world.Set(entity, CYGNavAgent.Create(playerId, fixedPosition));
                    break;
            }

            _viewsDirty = true;
            return entity;
        }

        #endregion

        #region Network Input

        /// <summary>
        /// Ingests an opcode 19 input tick packet. Framing:
        /// [0] opcode, [1..8] room, [9..12] tick (BE u32),
        /// [13..14] actor (BE u16), [15] payload length, [16..] inputs (9B each,
        /// newest first, optionally two redundant predecessors).
        /// </summary>
        public void HandleRemoteInputPacket(ArraySegment<byte> packet)
        {
            if (_session == null || packet.Array == null || packet.Count < 16)
            {
                return;
            }

            byte[] data = packet.Array;
            int offset = packet.Offset;
            int count = packet.Count;

            if (data[offset] != CYGNetwork.CYG_WIRE_INPUT_TICK)
            {
                return;
            }

            uint clientTick = ((uint)data[offset + 9] << 24)
                       | ((uint)data[offset + 10] << 16)
                       | ((uint)data[offset + 11] << 8)
                       | data[offset + 12];

            ushort actorId = (ushort)((data[offset + 13] << 8) | data[offset + 14]);
            byte inputLen = data[offset + 15];
            int payloadStart = offset + 16;

            if (actorId == 0
                || actorId == CYGNetwork.LocalActorNumber
                || actorId == _localPlayerId
                || clientTick > int.MaxValue)
            {
                return;
            }

            if (inputLen != CYGPlayerInput.SizeInBytes
                && inputLen != CYGPlayerInput.SizeInBytes * 2
                && inputLen != CYGPlayerInput.SizeInBytes * 3)
            {
                return;
            }

            if (count - (payloadStart - offset) != inputLen)
            {
                return;
            }

            CYGPlayerInput current = CYGPlayerInput.ReadFrom(data, payloadStart);
            if (current.Tick != (int)clientTick)
            {
                return;
            }

            bool hasPrev1 = inputLen >= CYGPlayerInput.SizeInBytes * 2;
            bool hasPrev2 = inputLen >= CYGPlayerInput.SizeInBytes * 3;
            CYGPlayerInput prev1 = default;
            CYGPlayerInput prev2 = default;

            if (hasPrev1)
            {
                prev1 = CYGPlayerInput.ReadFrom(data, payloadStart + CYGPlayerInput.SizeInBytes);
                if (prev1.Tick != current.Tick - 1)
                {
                    return;
                }
            }
            if (hasPrev2)
            {
                prev2 = CYGPlayerInput.ReadFrom(data, payloadStart + CYGPlayerInput.SizeInBytes * 2);
                if (prev2.Tick != current.Tick - 2)
                {
                    return;
                }
            }

            _session.OnRemoteInputReceived(actorId, current);
            if (hasPrev1) _session.OnRemoteInputReceived(actorId, prev1);
            if (hasPrev2) _session.OnRemoteInputReceived(actorId, prev2);

            if (spawnRemotePlayersOnInput && !TryFindEntityByPlayerId(actorId, out _))
            {
                SpawnPlayer(actorId, remoteSpawnPosition);
            }
        }

        /// <summary>
        /// Applies one remote input directly. Used by local bots and tests.
        /// Returns true when the input caused a rollback.
        /// </summary>
        public bool InjectPlayerInput(int playerId, in CYGPlayerInput input)
        {
            return _session != null && _session.OnRemoteInputReceived(playerId, input);
        }

        public bool TryFindEntityByPlayerId(int playerId, out CYGEntityRef entity)
        {
            if (_world != null)
            {
                for (int i = 0; i < _world.SlotCount; i++)
                {
                    CYGEntityRef candidate;
                    if (!_world.TryGetEntityAt(i, out candidate))
                    {
                        continue;
                    }

                    CYGSimAgent agent;
                    if (_world.TryGet(candidate, out agent) && agent.PlayerId == playerId)
                    {
                        entity = candidate;
                        return true;
                    }

                    CYGNavAgent nav;
                    if (_world.TryGet(candidate, out nav) && nav.PlayerId == playerId)
                    {
                        entity = candidate;
                        return true;
                    }

                    CYGPhysicsBody3D body3D;
                    if (_world.TryGet(candidate, out body3D) && body3D.PlayerId == playerId)
                    {
                        entity = candidate;
                        return true;
                    }

                    CYGPhysicsBody2D body2D;
                    if (_world.TryGet(candidate, out body2D) && body2D.PlayerId == playerId)
                    {
                        entity = candidate;
                        return true;
                    }
                }
            }

            entity = CYGEntityRef.None;
            return false;
        }

        private void HandleRollbackOccurred(int fromTick, int toTick)
        {
            _snapViews = true;
            if (callbacks != null)
            {
                callbacks.NotifyRollback(fromTick, toTick);
            }
        }

        private CYGNavMeshAuthoring FindNavMeshAuthoring()
        {
#if UNITY_2022_2_OR_NEWER
            return FindFirstObjectByType<CYGNavMeshAuthoring>();
#else
            return FindObjectOfType<CYGNavMeshAuthoring>();
#endif
        }

        #endregion

        #region Debug GUI

        private void OnGUI()
        {
            if (!showDebugStats || _session == null) return;

            GUILayout.BeginArea(new Rect(10, 10, 360, 320), GUI.skin.box);
            GUILayout.Label($"<b>CookYourGame ECS Sim</b> ({ticksPerSecond}Hz, {simulationMode})");
            GUILayout.Label($"Sim Tick: {CurrentSimTick}  Confirmed: {ConfirmedTick}");
            GUILayout.Label($"Prediction: {_session.PredictionLead} / {maxPredictionTicks} ticks  Authority: {syncAuthority}");
            GUILayout.Label($"Entities: {ActiveEntities}  Views: {ViewCount}");
            GUILayout.Label($"Checksum: 0x{Checksum:X8}");
            GUILayout.Label($"Rollbacks: {_session.TotalRollbacks}  Resim: {_session.TotalResimulatedTicks} ticks");
            GUILayout.Label($"Rejected input: {_session.RejectedInputs}  Dropped rollback: {_session.RejectedRollbacks}");
            GUILayout.Label($"Local Player: {_localPlayerId}  Entity: {_localEntity.Index}v{_localEntity.Version}");
            if (_navMesh != null)
            {
                GUILayout.Label($"NavMesh: {_navMesh.TriangleCount} tris 0x{_navMesh.Checksum:X8}");
            }
            GUILayout.Label($"UDP: {(CYGNetwork.IsConnected ? "Connected" : "Disconnected")} ({CYGNetwork.Latency:F1}ms)");
            GUILayout.EndArea();
        }

        #endregion
    }
}
