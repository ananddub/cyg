using System;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Presentation-layer owner for entity views. It builds the view registry
    /// with pooling, prefab lookup and view context from serialized settings,
    /// relays spawn/despawn/binding events, and can prewarm pooled instances.
    /// When present on the runner's GameObject it is adopted automatically.
    /// </summary>
    [DisallowMultipleComponent]
    [AddComponentMenu("CookYourGame/Sim/CYG Entity View Updater")]
    public class CYGEntityViewUpdater : CYGMonoBehaviour
    {
        [Header("View Prefabs")]
        [Tooltip("Prefab used when an entity has no explicit prefab or prefab key.")]
        [SerializeField] private GameObject defaultPrefab;

        [Tooltip("Key based prefab lookup used by CYGEntityAuthoring.ViewPrefabKey.")]
        [SerializeField] private CYGEntityViewPrefabRegistry prefabRegistry;

        [Header("View Context")]
        [Tooltip("Scene references injected into every spawned entity view.")]
        [SerializeField] private CYGViewContext viewContext;

        [Header("Root")]
        [Tooltip("Parent for spawned views. Defaults to this transform.")]
        [SerializeField] private Transform viewRoot;

        [Header("Pooling")]
        [SerializeField] private bool pooling = true;
        [SerializeField] private int maxPooledPerPrefab = 64;
        [SerializeField] private bool prewarmOnStart = true;

        private CYGViewRegistry _registry;
        private bool _attached;

        #region Properties

        public CYGViewRegistry Registry => _registry;
        public new CYGViewContext ViewContext
        {
            get => viewContext;
            set
            {
                viewContext = value;
                if (_registry != null)
                {
                    _registry.ViewContext = value;
                }
            }
        }
        public CYGEntityViewPrefabRegistry PrefabRegistry
        {
            get => prefabRegistry;
            set
            {
                prefabRegistry = value;
                if (_registry != null)
                {
                    _registry.PrefabRegistry = value;
                }
            }
        }

        public GameObject DefaultPrefab
        {
            get => defaultPrefab;
            set
            {
                defaultPrefab = value;
                if (_registry != null)
                {
                    _registry.DefaultPrefab = value;
                }
            }
        }

        public bool IsAttached => _attached;
        public int ViewCount => _registry != null ? _registry.ViewCount : 0;
        public int PooledViewCount => _registry != null ? _registry.PooledViewCount : 0;
        public int ViewsSpawnedFromPool => _registry != null ? _registry.ViewsSpawnedFromPool : 0;
        public CYGEntityView LocalView => _registry != null ? _registry.LocalView : null;

        public bool PoolingEnabled
        {
            get => pooling;
            set
            {
                pooling = value;
                if (_registry != null)
                {
                    _registry.PoolingEnabled = value;
                }
            }
        }

        public int MaxPooledPerPrefab
        {
            get => maxPooledPerPrefab;
            set
            {
                maxPooledPerPrefab = value;
                if (_registry != null)
                {
                    _registry.MaxPooledPerPrefab = value;
                }
            }
        }

        public event Action<CYGEntityView> ViewSpawned;
        public event Action<CYGEntityView> ViewDespawned;
        public event Action<CYGEntityView> ViewBound;
        public event Action<CYGEntityView> LocalViewChanged;

        #endregion

        /// <summary>
        /// Creates (or returns) the registry with this updater's settings and
        /// attaches it to <paramref name="owner"/>. Called by the runner during
        /// initialization, or by late attachment through the updater's Start.
        /// </summary>
        public CYGViewRegistry CreateOrGetRegistry(
            CYGDeterministicRunner owner,
            GameObject fallbackPrefab)
        {
            bool created = false;
            if (_registry == null)
            {
                created = true;
                Transform root = viewRoot != null ? viewRoot : transform;
                _registry = new CYGViewRegistry(
                    root,
                    defaultPrefab != null ? defaultPrefab : fallbackPrefab);
                _registry.Owner = this;
                _registry.ViewContext = viewContext;
                _registry.PrefabRegistry = prefabRegistry;
                _registry.PoolingEnabled = pooling;
                _registry.MaxPooledPerPrefab = maxPooledPerPrefab;
                _registry.ViewSpawned += HandleViewSpawned;
                _registry.ViewDespawned += HandleViewDespawned;
                _registry.ViewBound += HandleViewBound;
                _registry.LocalViewChanged += HandleLocalViewChanged;
            }

            if (viewContext == null && owner != null)
            {
                viewContext = owner.ViewContext;
                if (_registry.ViewContext == null)
                {
                    _registry.ViewContext = viewContext;
                }
            }

            _attached = owner != null;
            if (created && prewarmOnStart)
            {
                Prewarm();
            }
            return _registry;
        }

        /// <summary>Instantiates inactive instances for every prewarm entry.</summary>
        public void Prewarm()
        {
            if (_registry == null || prefabRegistry == null)
            {
                return;
            }

            for (int i = 0; i < prefabRegistry.AssetCount; i++)
            {
                CYGEntityViewPrefabAsset asset = prefabRegistry.GetAssetAt(i);
                if (asset != null && asset.Pool && asset.Prewarm > 0)
                {
                    _registry.Prewarm(asset.Prefab, asset.Prewarm);
                }
            }
        }

        private void Start()
        {
            if (_registry == null)
            {
                CYGDeterministicRunner owner;
                if (TryGetRunner(out owner))
                {
                    owner.AttachViewUpdater(this);
                }
                else
                {
                    Debug.LogWarning(
                        "[CYG] CYGEntityViewUpdater has no CYGDeterministicRunner. " +
                        "Add a runner to the scene or assign Runner Override; views will not spawn.",
                        this);
                }
            }
        }

        protected override void OnDestroy()
        {
            if (_registry == null)
            {
                base.OnDestroy();
                return;
            }

            _registry.ViewSpawned -= HandleViewSpawned;
            _registry.ViewDespawned -= HandleViewDespawned;
            _registry.ViewBound -= HandleViewBound;
            _registry.LocalViewChanged -= HandleLocalViewChanged;
            _registry = null;
            _attached = false;
            base.OnDestroy();
        }

        private void HandleViewSpawned(CYGEntityView view) => ViewSpawned?.Invoke(view);
        private void HandleViewDespawned(CYGEntityView view) => ViewDespawned?.Invoke(view);
        private void HandleViewBound(CYGEntityView view) => ViewBound?.Invoke(view);
        private void HandleLocalViewChanged(CYGEntityView view) => LocalViewChanged?.Invoke(view);
    }
}
