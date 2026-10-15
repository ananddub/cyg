using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame
{
    /// <summary>Registry of prefabs that may be instantiated over the network.</summary>
    [CreateAssetMenu(fileName = "CYGPrefabRegistry", menuName = "CookYourGame/Prefab Registry", order = 1)]
    public class CYGPrefabRegistry : ScriptableObject
    {
        const ushort InvalidPrefabId = ushort.MaxValue;

        [Tooltip("Prefabs spawnable by name; the list index is the wire prefab id")]
        [SerializeField] List<GameObject> networkPrefabs = new List<GameObject>();

        Dictionary<string, ushort> idByName;

        /// <summary>Snapshot count of registered prefabs.</summary>
        public int Count
        {
            get { return networkPrefabs.Count; }
        }

        /// <summary>Resolves a prefab name to its wire id, or ushort.MaxValue when unknown.</summary>
        public ushort GetPrefabId(string name)
        {
            if (string.IsNullOrEmpty(name))
            {
                Debug.LogError("[CYG] GetPrefabId requires a prefab name.");
                return InvalidPrefabId;
            }

            EnsureIndex();

            ushort id;
            if (idByName.TryGetValue(name, out id))
            {
                return id;
            }

            Debug.LogError("[CYG] prefab \"" + name + "\" is not registered in the prefab registry.");
            return InvalidPrefabId;
        }

        /// <summary>Resolves a wire id back to its prefab asset.</summary>
        public GameObject GetPrefab(ushort id)
        {
            if (id >= (ushort)networkPrefabs.Count)
            {
                Debug.LogError("[CYG] no prefab registered for id " + id + ".");
                return null;
            }

            GameObject prefab = networkPrefabs[id];
            if (prefab == null)
            {
                Debug.LogError("[CYG] prefab id " + id + " points at a missing asset.");
            }

            return prefab;
        }

        /// <summary>Adds a prefab if absent and returns its wire id.</summary>
        public ushort RegisterPrefab(GameObject prefab)
        {
            if (prefab == null)
            {
                Debug.LogError("[CYG] cannot register a null prefab.");
                return InvalidPrefabId;
            }

            EnsureIndex();

            ushort existing;
            if (idByName.TryGetValue(prefab.name, out existing))
            {
                return existing;
            }

            networkPrefabs.Add(prefab);
            idByName[prefab.name] = (ushort)(networkPrefabs.Count - 1);
            return (ushort)(networkPrefabs.Count - 1);
        }

        /// <summary>Adds a prefab at a specific id, growing the list with holes when needed.</summary>
        public ushort RegisterPrefab(GameObject prefab, ushort id)
        {
            if (prefab == null)
            {
                Debug.LogError("[CYG] cannot register a null prefab.");
                return InvalidPrefabId;
            }

            while ((ushort)networkPrefabs.Count <= id && networkPrefabs.Count < ushort.MaxValue)
            {
                networkPrefabs.Add(null);
            }

            if (id >= (ushort)networkPrefabs.Count)
            {
                Debug.LogError("[CYG] prefab id " + id + " is beyond the registry capacity.");
                return InvalidPrefabId;
            }

            networkPrefabs[id] = prefab;
            idByName = null;
            EnsureIndex();
            return id;
        }

        /// <summary>Drops the lazily built name index, e.g. after editing the list elsewhere.</summary>
        public void InvalidateIndex()
        {
            idByName = null;
        }

        void EnsureIndex()
        {
            if (idByName != null)
            {
                return;
            }

            idByName = new Dictionary<string, ushort>(networkPrefabs.Count);
            for (int i = 0; i < networkPrefabs.Count; i++)
            {
                GameObject prefab = networkPrefabs[i];
                if (prefab == null)
                {
                    continue;
                }

                // The first entry wins, so a duplicated name never re-targets an existing id.
                if (!idByName.ContainsKey(prefab.name))
                {
                    idByName.Add(prefab.name, (ushort)i);
                }
            }
        }
    }
}
