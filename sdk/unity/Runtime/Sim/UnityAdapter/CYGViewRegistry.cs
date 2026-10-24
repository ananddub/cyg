using System;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Maps deterministic ECS entities to Unity views. The registry reads
    /// authoritative transform state straight out of the world each tick
    /// (navigation, 3D physics, 2D physics, then the built-in movement agent,
    /// in that precedence order) and drives prefab instances through a pool.
    ///
    /// A view is created for every entity with a player id, an explicit
    /// prefab binding, or a deterministic <see cref="CYGViewBinding"/>. Static
    /// props are only visualized when scene authoring supplied a prefab/key.
    /// </summary>
    public sealed class CYGViewRegistry
    {
        private readonly Dictionary<CYGEntityRef, CYGEntityView> _views =
            new Dictionary<CYGEntityRef, CYGEntityView>();
        private readonly Dictionary<CYGEntityRef, GameObject> _prefabs =
            new Dictionary<CYGEntityRef, GameObject>();
        private readonly Dictionary<CYGEntityRef, string> _prefabKeys =
            new Dictionary<CYGEntityRef, string>();
        private readonly Dictionary<CYGEntityView, GameObject> _viewPrefabs =
            new Dictionary<CYGEntityView, GameObject>();
        private readonly HashSet<CYGEntityView> _owned = new HashSet<CYGEntityView>();
        private readonly List<CYGEntityRef> _stale = new List<CYGEntityRef>();
        private readonly HashSet<CYGEntityRef> _present = new HashSet<CYGEntityRef>();
        private readonly CYGViewPool _pool = new CYGViewPool();
        private readonly Transform _root;
        private GameObject _defaultPrefab;

        private CYGWorld _world;
        private CYGEntityView _localView;

        public CYGViewRegistry(Transform root, GameObject defaultPrefab)
        {
            _root = root;
            _defaultPrefab = defaultPrefab;
        }

        public Transform Root => _root;

        /// <summary>Fallback prefab used when an entity has no binding.</summary>
        public GameObject DefaultPrefab
        {
            get => _defaultPrefab;
            set => _defaultPrefab = value;
        }

        public int ViewCount => _views.Count;

        public int PooledViewCount => _pool.PooledCount;

        public int ViewsSpawnedFromPool => _pool.SpawnedFromPool;

        /// <summary>Scene-reference context injected into every spawned view.</summary>
        public CYGViewContext ViewContext { get; set; }

        /// <summary>Optional key based prefab lookup used before the default prefab.</summary>
        public CYGEntityViewPrefabRegistry PrefabRegistry { get; set; }

        public CYGEntityViewUpdater Owner { get; set; }

        public bool PoolingEnabled
        {
            get => _pool.Enabled;
            set => _pool.Enabled = value;
        }

        public int MaxPooledPerPrefab
        {
            get => _pool.MaxPerPrefab;
            set => _pool.MaxPerPrefab = value;
        }

        public int LocalPlayerId { get; private set; }
        public CYGEntityView LocalView => _localView;

        public event Action<CYGEntityView> ViewSpawned;
        public event Action<CYGEntityView> ViewDespawned;
        public event Action<CYGEntityView> ViewBound;
        public event Action<CYGEntityView> LocalViewChanged;

        /// <summary>Overrides the prefab used for one entity (scene authoring).</summary>
        public void SetPrefab(CYGEntityRef entity, GameObject prefab)
        {
            if (prefab == null)
            {
                _prefabs.Remove(entity);
            }
            else
            {
                _prefabs[entity] = prefab;
            }
        }

        /// <summary>Maps an entity to a key in <see cref="PrefabRegistry"/>.</summary>
        public void SetPrefabKey(CYGEntityRef entity, string key)
        {
            if (string.IsNullOrEmpty(key))
            {
                _prefabKeys.Remove(entity);
            }
            else
            {
                _prefabKeys[entity] = key;
            }
        }

        /// <summary>
        /// Writes a deterministic <see cref="CYGViewBinding"/> binding into the world.
        /// Prefer this over the presentation-only string key when the view must
        /// be recreated from a snapshot or created by a simulation system.
        /// </summary>
        public void SetViewPrefab(
            CYGEntityRef entity,
            CYGAssetId prefab,
            byte createBehaviour = CYGViewBinding.BindVerified,
            byte flags = CYGViewBinding.FlagPool | CYGViewBinding.FlagVisible)
        {
            if (_world == null)
            {
                throw new InvalidOperationException(
                    "CYGViewRegistry.SetViewPrefab requires the registry to be synced to a world first.");
            }

            if (!_world.Exists(entity))
            {
                throw new ArgumentException("Entity does not exist in the bound world.", nameof(entity));
            }

            EnsureViewStore(_world);
            _world.Set(entity, CYGViewBinding.Create(prefab, createBehaviour, flags));
        }

        public bool TryGetViewDefinition(CYGEntityRef entity, out CYGViewBinding view)
        {
            view = default(CYGViewBinding);
            return _world != null && _world.TryGet(entity, out view);
        }

        private static void EnsureViewStore(CYGWorld world)
        {
            ICYGComponentStore store;
            if (!world.TryGetStore(CYGViewBinding.TypeId, out store))
            {
                world.Register<CYGViewBinding>();
            }
        }

        /// <summary>Registers an existing scene view for an entity.</summary>
        public CYGEntityView Register(CYGEntityRef entity, CYGEntityView view)
        {
            if (view == null)
            {
                throw new ArgumentNullException(nameof(view));
            }

            _views[entity] = view;
            int playerId = ReadPlayerId(entity);
            view.BindEntity(entity, _world, ViewContext, Owner);
            if (_world != null && _world.TryGet(entity, out CYGViewBinding definition))
            {
                view.SetDefinition(definition);
            }
            view.Bind(playerId);
            view.SetViewFlags(false, LocalPlayerId > 0 && playerId == LocalPlayerId);
            if (!view.IsActive)
            {
                view.ActivateView();
            }
            ViewBound?.Invoke(view);
            if (LocalPlayerId > 0 && view.PlayerId == LocalPlayerId)
            {
                SetLocalView(view);
            }
            return view;
        }

        public bool TryGet(CYGEntityRef entity, out CYGEntityView view)
        {
            return _views.TryGetValue(entity, out view);
        }

        public bool TryGetView(CYGEntityRef entity, out CYGEntityView view)
        {
            return TryGet(entity, out view);
        }

        public bool TryGetView(int playerId, out CYGEntityView view)
        {
            if (playerId > 0)
            {
                foreach (KeyValuePair<CYGEntityRef, CYGEntityView> pair in _views)
                {
                    if (pair.Value != null && pair.Value.PlayerId == playerId)
                    {
                        view = pair.Value;
                        return true;
                    }
                }
            }
            view = null;
            return false;
        }

        public bool TryGetLocalView(out CYGEntityView view)
        {
            view = _localView;
            return view != null;
        }

        /// <summary>
        /// Applies the latest ECS state to every live view and tears down views
        /// whose entity disappeared (despawn, world clear, snapshot reset).
        /// <paramref name="snap"/> disables interpolation for one frame, which
        /// is what a rollback correction needs.
        /// </summary>
        public void Sync(
            CYGWorld world,
            float tickInterval,
            bool snap = false,
            int localPlayerId = 0,
            bool predicted = false)
        {
            if (world == null)
            {
                throw new ArgumentNullException(nameof(world));
            }

            CYGWorld previousWorld = _world;
            _world = world;
            LocalPlayerId = localPlayerId;
            _present.Clear();
            bool gameChanged = previousWorld != null && !ReferenceEquals(previousWorld, world);

            for (int i = 0; i < world.SlotCount; i++)
            {
                CYGEntityRef entity;
                if (!world.TryGetEntityAt(i, out entity))
                {
                    continue;
                }

                CYGSimAgent agent;
                bool hasAgent = world.TryGet<CYGSimAgent>(entity, out agent);
                CYGNavAgent nav;
                bool hasNav = world.TryGet<CYGNavAgent>(entity, out nav);
                CYGPhysicsBody3D body3D;
                bool has3D = world.TryGet<CYGPhysicsBody3D>(entity, out body3D);
                CYGPhysicsBody2D body2D;
                bool has2D = world.TryGet<CYGPhysicsBody2D>(entity, out body2D);

                int playerId = hasAgent ? agent.PlayerId : 0;
                FPVector3 position = hasAgent ? agent.Position : FPVector3.Zero;
                FPQuaternion rotation = hasAgent ? agent.Rotation : FPQuaternion.Identity;
                FPVector3 velocity = hasAgent ? agent.Velocity : FPVector3.Zero;
                int health = hasAgent ? agent.Health : 0;

                if (hasNav)
                {
                    playerId = nav.PlayerId != 0 ? nav.PlayerId : playerId;
                    position = nav.Position;
                    velocity = nav.Velocity;
                }
                else if (has3D)
                {
                    playerId = body3D.PlayerId != 0 ? body3D.PlayerId : playerId;
                    position = body3D.Position;
                    rotation = body3D.Rotation;
                    velocity = body3D.LinearVelocity;
                }
                else if (has2D)
                {
                    playerId = body2D.PlayerId != 0 ? body2D.PlayerId : playerId;
                    position = new FPVector3(body2D.Position.x, body2D.Position.y, FP.Zero);
                    rotation = FPQuaternion.Euler(FP.Zero, FP.Zero, body2D.Angle * FP.Rad2Deg);
                    velocity = new FPVector3(body2D.Velocity.x, body2D.Velocity.y, FP.Zero);
                }

                _prefabs.TryGetValue(entity, out GameObject explicitPrefab);
                _prefabKeys.TryGetValue(entity, out string prefabKey);

                CYGEntityViewPrefabAsset prefabAsset = null;
                GameObject registryPrefab = null;
                if (!string.IsNullOrEmpty(prefabKey) && PrefabRegistry != null)
                {
                    prefabAsset = PrefabRegistry.ResolveAsset(prefabKey);
                    if (prefabAsset != null)
                    {
                        registryPrefab = prefabAsset.Prefab;
                    }
                }

                CYGViewBinding definition = default(CYGViewBinding);
                bool hasDefinition = world.TryGet<CYGViewBinding>(entity, out definition);
                if (registryPrefab == null && hasDefinition && PrefabRegistry != null)
                {
                    prefabAsset = PrefabRegistry.ResolveAsset(definition.Prefab);
                    if (prefabAsset != null)
                    {
                        registryPrefab = prefabAsset.Prefab;
                    }
                }

                GameObject prefab = explicitPrefab != null ? explicitPrefab : registryPrefab;
                bool hasPrefab = prefab != null;
                bool hasTransform = hasAgent || hasNav || has3D || has2D;
                bool hasDefinitionPrefab = hasDefinition && !definition.Prefab.IsNone;

                // Custom components can own a view through CYGViewBinding even when
                // they do not use one of the built-in transform components.
                if (!hasTransform && !hasPrefab && !hasDefinitionPrefab)
                {
                    continue;
                }

                if (playerId <= 0 && !hasPrefab && !hasDefinitionPrefab)
                {
                    continue;
                }

                _present.Add(entity);

                bool poolable = hasDefinition
                    ? (definition.Flags & CYGViewBinding.FlagPool) != 0
                    : prefabAsset == null || prefabAsset.Pool;
                bool isLocal = localPlayerId > 0 && playerId == localPlayerId;
                CYGEntityView view;
                bool isNew = !_views.TryGetValue(entity, out view) || view == null;
                if (isNew)
                {
                    view = CreateView(
                        entity,
                        playerId,
                        prefab != null ? prefab : _defaultPrefab,
                        position,
                        poolable,
                        predicted,
                        isLocal,
                        definition,
                        hasDefinition);
                }
                else
                {
                    view.BindEntity(entity, world, ViewContext, Owner);
                    view.SetViewFlags(predicted, isLocal);
                    if (hasDefinition)
                    {
                        view.SetDefinition(definition);
                    }
                }

                if (!isNew && view.PlayerId != playerId)
                {
                    view.Bind(playerId);
                    ViewBound?.Invoke(view);
                }

                view.ApplyState(position, rotation, velocity, health, tickInterval, snap);

                if (isLocal)
                {
                    SetLocalView(view);
                }

                if (isNew)
                {
                    ViewBound?.Invoke(view);
                    ViewSpawned?.Invoke(view);
                }
            }

            if (_localView != null
                && (!_present.Contains(_localView.Entity) || !_localView.IsLocalPlayer))
            {
                SetLocalView(null);
            }

            _stale.Clear();
            foreach (KeyValuePair<CYGEntityRef, CYGEntityView> pair in _views)
            {
                if (!_present.Contains(pair.Key))
                {
                    _stale.Add(pair.Key);
                }
            }

            for (int i = 0; i < _stale.Count; i++)
            {
                Remove(_stale[i]);
            }

            if (gameChanged)
            {
                foreach (KeyValuePair<CYGEntityRef, CYGEntityView> pair in _views)
                {
                    if (pair.Value != null)
                    {
                        pair.Value.GameChanged();
                    }
                }
            }
        }

        public void Remove(CYGEntityRef entity)
        {
            if (_views.TryGetValue(entity, out CYGEntityView view))
            {
                _views.Remove(entity);

                if (view != null)
                {
                    ViewDespawned?.Invoke(view);
                    view.DeactivateView();

                    if (_localView == view)
                    {
                        SetLocalView(null);
                    }

                    if (_owned.Remove(view))
                    {
                        bool poolable = _viewPrefabs.TryGetValue(view, out GameObject prefab);
                        _viewPrefabs.Remove(view);

                        if (!poolable || !_pool.Return(view, prefab, MaxPooledPerPrefab))
                        {
                            DestroyView(view.gameObject);
                        }
                    }
                    else
                    {
                        // Scene-owned views are deactivated but not destroyed;
                        // the scene object controls its own lifetime.
                        view.gameObject.SetActive(false);
                    }
                }
            }

            _prefabs.Remove(entity);
            _prefabKeys.Remove(entity);
        }

        public void Clear(bool destroyGameObjects)
        {
            foreach (KeyValuePair<CYGEntityRef, CYGEntityView> pair in _views)
            {
                CYGEntityView view = pair.Value;
                if (view != null)
                {
                    ViewDespawned?.Invoke(view);
                    view.DeactivateView();
                }
            }

            if (destroyGameObjects)
            {
                foreach (KeyValuePair<CYGEntityRef, CYGEntityView> pair in _views)
                {
                    CYGEntityView view = pair.Value;
                    if (view != null && !_owned.Contains(view))
                    {
                        DestroyView(view.gameObject);
                    }
                }
                foreach (CYGEntityView view in _owned)
                {
                    if (view != null)
                    {
                        DestroyView(view.gameObject);
                    }
                }
            }

            _views.Clear();
            _prefabs.Clear();
            _prefabKeys.Clear();
            _viewPrefabs.Clear();
            _owned.Clear();
            _pool.Clear(destroyGameObjects);
            SetLocalView(null);
        }

        /// <summary>
        /// Copies per-entity prefab bindings into another registry. Used when
        /// an entity view updater is attached after the runner initialized.
        /// </summary>
        public void CopyBindingsTo(CYGViewRegistry other)
        {
            if (other == null)
            {
                return;
            }

            foreach (KeyValuePair<CYGEntityRef, GameObject> pair in _prefabs)
            {
                other._prefabs[pair.Key] = pair.Value;
            }
            foreach (KeyValuePair<CYGEntityRef, string> pair in _prefabKeys)
            {
                other._prefabKeys[pair.Key] = pair.Value;
            }
            if (other.ViewContext == null)
            {
                other.ViewContext = ViewContext;
            }
            if (other.PrefabRegistry == null)
            {
                other.PrefabRegistry = PrefabRegistry;
            }
        }

        /// <summary>Creates inactive pooled instances ahead of gameplay.</summary>
        public void Prewarm(GameObject prefab, int count)
        {
            if (!PoolingEnabled || count <= 0)
            {
                return;
            }

            for (int i = 0; i < count; i++)
            {
                GameObject instance = CreateInstance(prefab, Vector3.zero);
                CYGEntityView view = instance.GetComponent<CYGEntityView>();
                if (view == null)
                {
                    view = instance.AddComponent<CYGEntityView>();
                }
                instance.SetActive(false);
                if (!_pool.Return(view, prefab, MaxPooledPerPrefab))
                {
                    DestroyView(instance);
                    return;
                }
            }
        }

        private CYGEntityView CreateView(
            CYGEntityRef entity,
            int playerId,
            GameObject prefab,
            FPVector3 position,
            bool poolable,
            bool predicted,
            bool isLocalPlayer,
            CYGViewBinding definition,
            bool hasDefinition)
        {
            Vector3 spawn = new Vector3(
                position.x.AsFloat(),
                position.y.AsFloat(),
                position.z.AsFloat());

            CYGEntityView view;
            if (!poolable || !_pool.TryTake(prefab, _root, spawn, out view))
            {
                GameObject instance = CreateInstance(prefab, spawn);
                instance.SetActive(true);
                view = instance.GetComponent<CYGEntityView>();
                if (view == null)
                {
                    view = instance.AddComponent<CYGEntityView>();
                }
            }

            _owned.Add(view);
            if (poolable)
            {
                _viewPrefabs[view] = prefab;
            }
            view.gameObject.name = playerId > 0
                ? $"EntityView_{playerId}"
                : $"EntityView_{entity.Index}";

            view.Bind(playerId);
            view.BindEntity(entity, _world, ViewContext, Owner);
            view.SetViewFlags(predicted, isLocalPlayer);
            if (hasDefinition)
            {
                view.SetDefinition(definition);
            }
            view.ActivateView();
            return view;
        }

        private GameObject CreateInstance(GameObject prefab, Vector3 position)
        {
            if (prefab != null)
            {
                return UnityEngine.Object.Instantiate(prefab, position, Quaternion.identity, _root);
            }

            GameObject instance = GameObject.CreatePrimitive(PrimitiveType.Cube);
            instance.transform.SetParent(_root, false);
            instance.transform.position = position;
            Collider collider = instance.GetComponent<Collider>();
            if (collider != null)
            {
                DestroyView(collider);
            }
            return instance;
        }

        private int ReadPlayerId(CYGEntityRef entity)
        {
            if (_world == null || entity.IsNone)
            {
                return 0;
            }

            if (_world.TryGet(entity, out CYGSimAgent agent))
            {
                return agent.PlayerId;
            }
            if (_world.TryGet(entity, out CYGNavAgent nav))
            {
                return nav.PlayerId;
            }
            if (_world.TryGet(entity, out CYGPhysicsBody3D body3D))
            {
                return body3D.PlayerId;
            }
            if (_world.TryGet(entity, out CYGPhysicsBody2D body2D))
            {
                return body2D.PlayerId;
            }
            return 0;
        }

        private void SetLocalView(CYGEntityView view)
        {
            if (ReferenceEquals(_localView, view))
            {
                return;
            }
            _localView = view;
            LocalViewChanged?.Invoke(view);
        }

        private static void DestroyView(UnityEngine.Object value)
        {
            if (Application.isPlaying)
            {
                UnityEngine.Object.Destroy(value);
            }
            else
            {
                UnityEngine.Object.DestroyImmediate(value);
            }
        }
    }
}
