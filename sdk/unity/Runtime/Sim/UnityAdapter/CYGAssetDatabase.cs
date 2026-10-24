using System;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>One asset-database row: stable id, lookup key and Unity asset.</summary>
    [Serializable]
    public sealed class CYGAssetEntry
    {
        [SerializeField] private int id;
        [SerializeField] private string key;
        [SerializeField] private UnityEngine.Object asset;

        public int Id => id;
        public string Key => string.IsNullOrEmpty(key) ? (asset != null ? asset.name : string.Empty) : key;
        public UnityEngine.Object Asset => asset;

        public void Configure(int stableId, string lookupKey, UnityEngine.Object value)
        {
            id = stableId;
            key = lookupKey;
            asset = value;
        }

        internal void EnsureId()
        {
            if (id == 0)
            {
                string resolvedKey = Key;
                if (!string.IsNullOrEmpty(resolvedKey))
                {
                    id = CYGAssetDatabase.StableId(resolvedKey);
                }
            }
        }
    }

    /// <summary>
    /// Central Unity-side asset database for simulation data: character stats,
    /// weapon tables, ability configs and balance values. Rows carry a stable
    /// integer id so gameplay systems can read the same data on the server,
    /// the client and in headless tests through <see cref="CYGAssetRegistry"/>.
    ///
    /// Only immutable runtime data belongs here. Do not store per-match state.
    /// </summary>
    [CreateAssetMenu(
        fileName = "CYGAssetDatabase",
        menuName = "CookYourGame/Sim/Asset Database",
        order = 9)]
    public sealed class CYGAssetDatabase : ScriptableObject
    {
        [SerializeField] private List<CYGAssetEntry> entries = new List<CYGAssetEntry>();

        [Tooltip("Log rows whose id collides with an earlier row.")]
        [SerializeField] private bool logDuplicateIds = true;

        public int Count => entries.Count;
        public IReadOnlyList<CYGAssetEntry> Entries => entries;

        /// <summary>Stable FNV-1a id derived from a content key.</summary>
        public static int StableId(string key)
        {
            return CYGAssetRegistry.StableId(key);
        }

        /// <summary>
        /// Copies every valid row into the simulation registry. Returns the
        /// number of rows that were registered. Call once during startup.
        /// </summary>
        public int Populate(CYGAssetRegistry registry = null)
        {
            registry ??= CYGAssetRegistry.Shared;
            int registered = 0;
            HashSet<int> seen = logDuplicateIds ? new HashSet<int>() : null;

            for (int i = 0; i < entries.Count; i++)
            {
                CYGAssetEntry entry = entries[i];
                if (entry == null || entry.Asset == null)
                {
                    continue;
                }

                entry.EnsureId();
                if (entry.Id == 0)
                {
                    continue;
                }

                if (seen != null && !seen.Add(entry.Id) && logDuplicateIds)
                {
                    Debug.LogWarning(
                        "[CYG] Asset database '" + name + "' has duplicate id " + entry.Id
                        + " at row " + i + " (" + entry.Key + ").", this);
                }

                if (registry.Register(entry.Id, entry.Asset, entry.Key))
                {
                    registered++;
                }
            }

            return registered;
        }

        public bool TryGet<T>(int id, out T asset) where T : UnityEngine.Object
        {
            CYGAssetEntry entry = Find(id);
            asset = entry != null ? entry.Asset as T : null;
            return asset != null;
        }

        public bool TryGet<T>(string key, out T asset) where T : UnityEngine.Object
        {
            CYGAssetEntry entry = Find(key);
            asset = entry != null ? entry.Asset as T : null;
            return asset != null;
        }

        public T Get<T>(int id) where T : UnityEngine.Object
        {
            return TryGet(id, out T asset) ? asset : null;
        }

        public T Get<T>(string key) where T : UnityEngine.Object
        {
            return TryGet(key, out T asset) ? asset : null;
        }

        public CYGAssetEntry Find(int id)
        {
            if (id == 0)
            {
                return null;
            }

            for (int i = 0; i < entries.Count; i++)
            {
                CYGAssetEntry entry = entries[i];
                if (entry == null)
                {
                    continue;
                }
                entry.EnsureId();
                if (entry.Id == id)
                {
                    return entry;
                }
            }
            return null;
        }

        public CYGAssetEntry Find(string key)
        {
            if (string.IsNullOrEmpty(key))
            {
                return null;
            }

            for (int i = 0; i < entries.Count; i++)
            {
                CYGAssetEntry entry = entries[i];
                if (entry != null && string.Equals(entry.Key, key, StringComparison.Ordinal))
                {
                    return entry;
                }
            }
            return null;
        }

        public CYGAssetEntry Register(int stableId, string key, UnityEngine.Object asset)
        {
            if (stableId == 0 && !string.IsNullOrEmpty(key))
            {
                stableId = StableId(key);
            }

            CYGAssetEntry entry = new CYGAssetEntry();
            entry.Configure(stableId, key, asset);
            entries.Add(entry);
            return entry;
        }

        private void OnValidate()
        {
            for (int i = 0; i < entries.Count; i++)
            {
                entries[i]?.EnsureId();
            }
        }
    }
}
