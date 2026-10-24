using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Simulation-side asset registry. Systems read balance data (character
    /// stats, weapon stats, ability configs) by a stable integer id without
    /// touching Unity objects, so the same lookup works in the Unity player,
    /// the headless server and the deterministic test tools.
    ///
    /// The registry is static content, not per-tick state: it is populated
    /// once at startup and is intentionally excluded from snapshots and the
    /// world checksum. Entries must therefore be immutable while a match runs.
    /// </summary>
    public sealed class CYGAssetRegistry
    {
        private readonly Dictionary<int, object> _byId = new Dictionary<int, object>();
        private readonly Dictionary<string, int> _byKey = new Dictionary<string, int>(StringComparer.Ordinal);

        /// <summary>Process-wide registry used by Unity adapters and systems.</summary>
        public static CYGAssetRegistry Shared { get; } = new CYGAssetRegistry();

        public int Count
        {
            get { return _byId.Count; }
        }

        public IReadOnlyCollection<int> Ids
        {
            get { return _byId.Keys; }
        }

        /// <summary>Registers an asset under an explicit id. Later entries win.</summary>
        public bool Register(int id, object value, string key = null)
        {
            if (id == 0 || value == null)
            {
                return false;
            }

            _byId[id] = value;
            if (!string.IsNullOrEmpty(key))
            {
                _byKey[key] = id;
            }
            return true;
        }

        /// <summary>
        /// Registers an asset by key. A zero id derives a stable FNV-1a id from
        /// the key so content-only projects do not need manual numbering.
        /// </summary>
        public int Register(string key, object value, int id = 0)
        {
            if (string.IsNullOrEmpty(key) || value == null)
            {
                return 0;
            }

            int resolved = id != 0 ? id : StableId(key);
            Register(resolved, value, key);
            return resolved;
        }

        public bool TryGet(int id, out object value)
        {
            return _byId.TryGetValue(id, out value);
        }

        public bool TryGet<T>(int id, out T value) where T : class
        {
            if (_byId.TryGetValue(id, out object stored) && stored is T typed)
            {
                value = typed;
                return true;
            }
            value = null;
            return false;
        }

        public bool TryGet<T>(CYGAssetId id, out T value) where T : class
        {
            return TryGet(id.Id, out value);
        }

        public bool TryGet<T>(string key, out T value) where T : class
        {
            if (!string.IsNullOrEmpty(key)
                && _byKey.TryGetValue(key, out int id)
                && _byId.TryGetValue(id, out object stored)
                && stored is T typed)
            {
                value = typed;
                return true;
            }
            value = null;
            return false;
        }

        public T Get<T>(int id) where T : class
        {
            return TryGet(id, out T value) ? value : null;
        }

        public T Get<T>(string key) where T : class
        {
            return TryGet(key, out T value) ? value : null;
        }

        public bool Contains(int id)
        {
            return _byId.ContainsKey(id);
        }

        public bool Remove(int id)
        {
            if (!_byId.Remove(id))
            {
                return false;
            }

            List<string> stale = null;
            foreach (KeyValuePair<string, int> pair in _byKey)
            {
                if (pair.Value == id)
                {
                    (stale ??= new List<string>()).Add(pair.Key);
                }
            }
            if (stale != null)
            {
                for (int i = 0; i < stale.Count; i++)
                {
                    _byKey.Remove(stale[i]);
                }
            }
            return true;
        }

        public void Clear()
        {
            _byId.Clear();
            _byKey.Clear();
        }

        /// <summary>
        /// Stable 32-bit FNV-1a id for a content key. Never returns zero so the
        /// value can be stored in <see cref="CYGAssetId.None"/>-aware fields.
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
    }
}
