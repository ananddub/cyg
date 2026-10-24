using System;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Describes how one entity view prefab is instantiated and pooled.
    /// Mirrors reference's entity view prefab asset: the key is what scene
    /// authoring or gameplay code uses to bind an entity to this view.
    /// </summary>
    [CreateAssetMenu(
        fileName = "CYGViewPrefab",
        menuName = "CookYourGame/Sim/Entity View Prefab",
        order = 11)]
    public class CYGEntityViewPrefabAsset : ScriptableObject
    {
        [Tooltip("Lookup key used by CYGEntityAuthoring.ViewPrefabKey. Defaults to the asset name.")]
        [SerializeField] private string key;

        [Tooltip("Stable deterministic id stored in the simulation CYGViewBinding component. Zero derives an id from Key.")]
        [SerializeField] private int assetId;

        [Tooltip("Prefab instantiated for the entity. Must contain a CYGEntityView (added automatically when missing).")]
        [SerializeField] private GameObject prefab;

        [Tooltip("Recycle instances through the view pool instead of destroying them.")]
        [SerializeField] private bool pool = true;

        [Tooltip("Instances created up front and kept inactive.")]
        [SerializeField] private int prewarm = 0;

        public string Key => string.IsNullOrEmpty(key) ? name : key;
        public int AssetId => assetId != 0 ? assetId : CYGEntityViewPrefabRegistry.StableId(Key);
        public GameObject Prefab => prefab;
        public bool Pool => pool;
        public int Prewarm => prewarm;

        public void Configure(
            string lookupKey,
            GameObject prefabInstance,
            bool usePool = true,
            int prewarmCount = 0,
            int stableId = 0)
        {
            key = lookupKey;
            assetId = stableId;
            prefab = prefabInstance;
            pool = usePool;
            prewarm = prewarmCount;
        }

        private void OnValidate()
        {
            if (prewarm < 0)
            {
                prewarm = 0;
            }
        }
    }

    /// <summary>
    /// Maps stable keys to entity view prefab assets so the view updater can
    /// spawn the right prefab without per-entity manual wiring. Authoring
    /// objects reference a key; runtime-spawned entities can use the same key
    /// through <see cref="CYGViewRegistry.SetPrefabKey"/>.
    /// </summary>
    [CreateAssetMenu(
        fileName = "CYGEntityViewPrefabRegistry",
        menuName = "CookYourGame/Sim/Entity View Prefab Registry",
        order = 12)]
    public class CYGEntityViewPrefabRegistry : ScriptableObject
    {
        [SerializeField] private List<CYGEntityViewPrefabAsset> assets = new List<CYGEntityViewPrefabAsset>();

        private Dictionary<string, CYGEntityViewPrefabAsset> _lookup;
        private Dictionary<int, CYGEntityViewPrefabAsset> _idLookup;

        public IReadOnlyList<CYGEntityViewPrefabAsset> Assets => assets;
        public int AssetCount => assets.Count;

        /// <summary>
        /// Stable 32-bit id derived from a prefab key. Used when an asset does
        /// not specify an explicit id, so key-based authoring still produces a
        /// deterministic value safe to store in <see cref="CYGViewBinding"/>.
        /// </summary>
        public static int StableId(string key)
        {
            if (string.IsNullOrEmpty(key))
            {
                return 0;
            }

            uint hash = 2166136261u;
            for (int i = 0; i < key.Length; i++)
            {
                hash ^= key[i];
                hash *= 16777619u;
            }

            int id = unchecked((int)hash);
            return id != 0 ? id : 1;
        }

        public CYGEntityViewPrefabAsset ResolveAsset(string key)
        {
            if (string.IsNullOrEmpty(key))
            {
                return null;
            }

            EnsureLookup();
            _lookup.TryGetValue(key, out CYGEntityViewPrefabAsset asset);
            return asset;
        }

        public CYGEntityViewPrefabAsset ResolveAsset(CYGAssetId id)
        {
            if (id.IsNone)
            {
                return null;
            }

            EnsureLookup();
            _idLookup.TryGetValue(id.Id, out CYGEntityViewPrefabAsset asset);
            return asset;
        }

        public GameObject Resolve(string key)
        {
            CYGEntityViewPrefabAsset asset = ResolveAsset(key);
            return asset != null ? asset.Prefab : null;
        }

        public GameObject Resolve(CYGAssetId id)
        {
            CYGEntityViewPrefabAsset asset = ResolveAsset(id);
            return asset != null ? asset.Prefab : null;
        }

        public CYGEntityViewPrefabAsset GetAssetAt(int index)
        {
            return index >= 0 && index < assets.Count ? assets[index] : null;
        }

        public void Register(CYGEntityViewPrefabAsset asset)
        {
            if (asset != null && !assets.Contains(asset))
            {
                assets.Add(asset);
                _lookup = null;
                _idLookup = null;
            }
        }

        private void EnsureLookup()
        {
            if (_lookup != null && _idLookup != null)
            {
                return;
            }

            _lookup = new Dictionary<string, CYGEntityViewPrefabAsset>(assets.Count, StringComparer.Ordinal);
            _idLookup = new Dictionary<int, CYGEntityViewPrefabAsset>(assets.Count);
            for (int i = 0; i < assets.Count; i++)
            {
                CYGEntityViewPrefabAsset asset = assets[i];
                if (asset == null)
                {
                    continue;
                }

                if (!string.IsNullOrEmpty(asset.Key))
                {
                    _lookup[asset.Key] = asset;
                }

                int id = asset.AssetId;
                if (id != 0)
                {
                    _idLookup[id] = asset;
                }
            }
        }

        private void OnValidate()
        {
            _lookup = null;
            _idLookup = null;
        }
    }
}
