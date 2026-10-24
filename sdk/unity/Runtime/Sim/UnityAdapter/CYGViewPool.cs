using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Recycles entity view GameObjects per prefab. Instances are deactivated
    /// on return and reactivated with their view components re-initialized,
    /// so spawning and despawning entities stops allocating during gameplay.
    /// </summary>
    internal sealed class CYGViewPool
    {
        private sealed class Bucket
        {
            public readonly Stack<CYGEntityView> Instances = new Stack<CYGEntityView>(8);
        }

        private readonly Dictionary<GameObject, Bucket> _buckets =
            new Dictionary<GameObject, Bucket>();
        private readonly Bucket _primitiveBucket = new Bucket();

        public bool Enabled { get; set; } = true;
        public int MaxPerPrefab { get; set; } = 64;
        public int SpawnedFromPool { get; private set; }

        public int PooledCount
        {
            get
            {
                int count = _primitiveBucket.Instances.Count;
                foreach (KeyValuePair<GameObject, Bucket> pair in _buckets)
                {
                    count += pair.Value.Instances.Count;
                }
                return count;
            }
        }

        public bool TryTake(
            GameObject prefab,
            Transform root,
            Vector3 position,
            out CYGEntityView view)
        {
            if (!Enabled)
            {
                view = null;
                return false;
            }

            Bucket bucket = prefab != null ? GetBucket(prefab) : _primitiveBucket;
            while (bucket.Instances.Count > 0)
            {
                view = bucket.Instances.Pop();
                if (view == null)
                {
                    continue;
                }

                GameObject instance = view.gameObject;
                instance.transform.SetParent(root, false);
                instance.transform.position = position;
                instance.transform.rotation = Quaternion.identity;
                view.PrepareForReuse();
                instance.SetActive(true);
                SpawnedFromPool++;
                return true;
            }

            view = null;
            return false;
        }

        public bool Return(CYGEntityView view, GameObject prefab, int maxPerPrefab)
        {
            if (!Enabled || view == null)
            {
                return false;
            }

            view.gameObject.SetActive(false);

            Bucket bucket = prefab != null ? GetBucket(prefab) : _primitiveBucket;
            int capacity = maxPerPrefab > 0 ? maxPerPrefab : MaxPerPrefab;
            if (bucket.Instances.Count >= capacity)
            {
                return false;
            }

            bucket.Instances.Push(view);
            return true;
        }

        public void Clear(bool destroyInstances)
        {
            ClearBucket(_primitiveBucket, destroyInstances);
            foreach (KeyValuePair<GameObject, Bucket> pair in _buckets)
            {
                ClearBucket(pair.Value, destroyInstances);
            }
            _buckets.Clear();
            SpawnedFromPool = 0;
        }

        private Bucket GetBucket(GameObject prefab)
        {
            if (!_buckets.TryGetValue(prefab, out Bucket bucket))
            {
                bucket = new Bucket();
                _buckets[prefab] = bucket;
            }
            return bucket;
        }

        private static void ClearBucket(Bucket bucket, bool destroyInstances)
        {
            while (bucket.Instances.Count > 0)
            {
                CYGEntityView view = bucket.Instances.Pop();
                if (view == null)
                {
                    continue;
                }

                if (destroyInstances)
                {
                    GameObject instance = view.gameObject;
                    if (Application.isPlaying)
                    {
                        Object.Destroy(instance);
                    }
                    else
                    {
                        Object.DestroyImmediate(instance);
                    }
                }
            }
        }
    }
}
