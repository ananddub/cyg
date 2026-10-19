using System;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.SceneManagement;

#if UNITY_6000_0_OR_NEWER
using InstanceIdType = UnityEngine.EntityId;
#else
using InstanceIdType = System.Int32;
#endif

namespace CookYourGame
{
    /// <summary>
    /// Static GameObject pool so networked spawn/despawn does not thrash the Unity GC.
    /// Instances are keyed by the prefab asset NAME (not the ushort wire id): the primary
    /// Spawn(string)/Despawn(GameObject) entry points both speak names, name keying keeps the
    /// spawn and despawn queues symmetric without a registry lookup on the hot path, and it stays
    /// correct across editor sessions where wire ids can shift while a name cannot.
    ///
    /// RESPAWN-IDENTITY CONTRACT: a pooled GameObject keeps the _viewId it was assigned on its
    /// previous life. The pool never clears or reassigns a view id (it has no knowledge of the
    /// network actor). The integration owner MUST reassign identity on every respawn by calling
    /// CYGView.AssignViewId before firing spawn hooks. Use SpawnView / Spawn with the out overload
    /// to receive the CYGView back so the caller can assign it.
    /// </summary>
    public static class CYGObjectPool
    {
        /// <summary>State a pooled instance is in, tracked on the component so idempotency does not rely on dictionaries alone.</summary>
        internal enum PooledState : byte
        {
            Spawned = 0,
            Pooled = 1,
        }

        const int DefaultMaxPerPrefab = 256;

        // Keyed by prefab asset name -> idle instances, oldest first. Read-only reference, mutable contents.
        static readonly Dictionary<string, Queue<CYGPooledInstance>> pools = new Dictionary<string, Queue<CYGPooledInstance>>();

        // Reverse lookup so Despawn(GameObject) is O(1); only currently-spawned instances live here.
        static readonly Dictionary<InstanceIdType, CYGPooledInstance> activeByInstance = new Dictionary<InstanceIdType, CYGPooledInstance>();

        static InstanceIdType GetInstanceKey(GameObject go)
        {
#if UNITY_6000_0_OR_NEWER
            return go.GetEntityId();
#else
            return go.GetInstanceID();
#endif
        }

        static GameObject poolRoot;
        static bool subscribed;
        static int activeCount;
        static int pooledCount;
        static int totalCreated;

        /// <summary>Idle instances retained per prefab before further despawns are destroyed instead of pooled. Default 256.</summary>
        public static int MaxPerPrefab = DefaultMaxPerPrefab;

        /// <summary>Number of instances handed out and not yet returned.</summary>
        public static int ActiveCount
        {
            get { return activeCount; }
        }

        /// <summary>Number of idle instances retained for reuse.</summary>
        public static int PooledCount
        {
            get { return pooledCount; }
        }

        /// <summary>Total instances the pool has instantiated this domain (lifetime, never reset by Clear).</summary>
        public static int TotalCreated
        {
            get { return totalCreated; }
        }

        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.BeforeSceneLoad)]
        static void Boot()
        {
            pools.Clear();
            activeByInstance.Clear();
            poolRoot = null;
            subscribed = false;
            activeCount = 0;
            pooledCount = 0;
            totalCreated = 0;
            EnsureSubscribed();
        }

        /// <summary>Pre-creates inactive clones of a registered prefab so the first spawns never allocate.</summary>
        public static void Prewarm(string prefabName, int count)
        {
            GameObject prefab = ResolvePrefab(prefabName);
            if (prefab == null)
            {
                return;
            }

            Prewarm(prefab, count);
        }

        /// <summary>Pre-creates inactive clones of a prefab so the first spawns never allocate.</summary>
        public static void Prewarm(GameObject prefab, int count)
        {
            if (prefab == null || count <= 0)
            {
                return;
            }

            EnsureSubscribed();
            string key = prefab.name;
            Queue<CYGPooledInstance> queue = GetOrCreateQueue(key);

            Transform root = EnsureRoot();
            for (int i = 0; i < count; i++)
            {
                // A prewarm must never blow past the retention cap.
                if (queue.Count >= MaxPerPrefab)
                {
                    break;
                }

                GameObject clone = UnityEngine.Object.Instantiate(prefab);
                clone.transform.SetParent(root, false);
                clone.SetActive(false);

                CYGPooledInstance instance = clone.AddComponent<CYGPooledInstance>();
                instance.PrefabKey = key;
                instance.State = PooledState.Pooled;
                instance.CachedView = clone.GetComponent<CYGView>();

                queue.Enqueue(instance);
                pooledCount++;
                totalCreated++;
            }
        }

        /// <summary>Hands out a pooled (or freshly created) instance of a registered prefab. The caller MUST AssignViewId before InvokeSpawnHooks.</summary>
        public static GameObject Spawn(string prefabName, Vector3 position, Quaternion rotation)
        {
            GameObject prefab = ResolvePrefab(prefabName);
            if (prefab == null)
            {
                return null;
            }

            return Spawn(prefab, position, rotation);
        }

        /// <summary>Hands out a pooled (or freshly created) instance and its cached CYGView through out parameters so the caller can reassign identity.</summary>
        public static GameObject Spawn(string prefabName, Vector3 position, Quaternion rotation, out CYGView view)
        {
            GameObject prefab = ResolvePrefab(prefabName);
            if (prefab == null)
            {
                view = null;
                return null;
            }

            GameObject result = SpawnInternal(prefab, prefab.name, position, rotation, true, out view);
            return result;
        }

        /// <summary>Hands out a pooled (or freshly created) instance of a prefab. The caller MUST AssignViewId before InvokeSpawnHooks.</summary>
        public static GameObject Spawn(GameObject prefab, Vector3 position, Quaternion rotation)
        {
            CYGView ignored;
            return SpawnInternal(prefab, prefab != null ? prefab.name : null, position, rotation, false, out ignored);
        }

        /// <summary>Hands out an instance and returns its CYGView so the caller can reassign identity (SpawnPrefab integration point).</summary>
        public static CYGView SpawnView(string prefabName, Vector3 position, Quaternion rotation)
        {
            GameObject prefab = ResolvePrefab(prefabName);
            if (prefab == null)
            {
                return null;
            }

            SpawnInternal(prefab, prefab.name, position, rotation, true, out CYGView view);
            return view;
        }

        /// <summary>Hands out a prefab instance and returns its CYGView so the caller can reassign identity.</summary>
        public static CYGView SpawnView(GameObject prefab, Vector3 position, Quaternion rotation)
        {
            if (prefab == null)
            {
                return null;
            }

            SpawnInternal(prefab, prefab.name, position, rotation, true, out CYGView view);
            return view;
        }

        /// <summary>Returns an instance to its pool (or destroys it when the retention cap is reached). False when the object is not pool-managed.</summary>
        public static bool Despawn(GameObject instance)
        {
            if (instance == null)
            {
                return false;
            }

            CYGPooledInstance tracked;
            if (!activeByInstance.TryGetValue(GetInstanceKey(instance), out tracked))
            {
                // Already returned, never spawned by us, or destroyed: a no-op, never a double-enqueue.
                return false;
            }

            return DespawnTracked(tracked, instance);
        }

        /// <summary>Returns the GameObject behind a view to its pool. False when the view is not pool-managed.</summary>
        public static bool Despawn(CYGView view)
        {
            if (view == null)
            {
                return false;
            }

            return Despawn(view.gameObject);
        }

        /// <summary>Destroys every retained idle instance, forgets still-active spawns and drops the hidden root.</summary>
        public static void Clear()
        {
            foreach (KeyValuePair<string, Queue<CYGPooledInstance>> entry in pools)
            {
                Queue<CYGPooledInstance> queue = entry.Value;
                CYGPooledInstance[] snapshot = queue.ToArray();
                for (int i = 0; i < snapshot.Length; i++)
                {
                    CYGPooledInstance instance = snapshot[i];
                    if (instance == null)
                    {
                        continue;
                    }

                    if (instance.gameObject != null)
                    {
                        UnityEngine.Object.Destroy(instance.gameObject);
                    }
                }

                queue.Clear();
            }

            pooledCount = 0;

            if (poolRoot != null)
            {
                UnityEngine.Object.Destroy(poolRoot);
                poolRoot = null;
            }

            // Still-active spawns are no longer tracked: a scene teardown lets Unity destroy them,
            // and an explicit Clear before a fresh domain must not leave stale reverse-lookup keys.
            activeByInstance.Clear();
            activeCount = 0;
        }

        static GameObject SpawnInternal(GameObject prefab, string key, Vector3 position, Quaternion rotation, bool ensureView, out CYGView view)
        {
            view = null;
            if (prefab == null || string.IsNullOrEmpty(key))
            {
                return null;
            }

            EnsureSubscribed();
            Queue<CYGPooledInstance> queue = GetOrCreateQueue(key);
            Transform root = EnsureRoot();

            CYGPooledInstance instance = DequeueValid(queue);
            if (instance != null)
            {
                // Reuse: reactivate and move first, then hand back the cached view for reassignment.
                instance.State = PooledState.Spawned;
                GameObject go = instance.gameObject;
                go.SetActive(true);
                go.transform.SetPositionAndRotation(position, rotation);
                activeByInstance[GetInstanceKey(go)] = instance;
                activeCount++;
                pooledCount--;

                if (ensureView)
                {
                    view = EnsureView(instance, go);
                }

                return go;
            }

            GameObject created = UnityEngine.Object.Instantiate(prefab, position, rotation);
            created.transform.SetParent(root, false);

            CYGPooledInstance fresh = created.AddComponent<CYGPooledInstance>();
            fresh.PrefabKey = key;
            fresh.State = PooledState.Spawned;
            fresh.CachedView = created.GetComponent<CYGView>();

            activeByInstance[GetInstanceKey(created)] = fresh;
            activeCount++;
            totalCreated++;

            if (ensureView)
            {
                view = EnsureView(fresh, created);
            }

            return created;
        }

        static bool DespawnTracked(CYGPooledInstance instance, GameObject gameObject)
        {
            if (instance.State == PooledState.Pooled)
            {
                return false;
            }

            InstanceIdType id = GetInstanceKey(gameObject);
            activeByInstance.Remove(id);

            // ORDER DECISION: deactivate, then fire OnDespawned hooks while the transform still holds
            // the last live pose so hooks can read it, and only THEN reset the transform to identity.
            gameObject.SetActive(false);
            if (instance.CachedView != null)
            {
                // InvokeDespawnHooks runs exactly once and unregisters the view from CYGViewTable itself.
                instance.CachedView.InvokeDespawnHooks();
            }

            Transform transform = gameObject.transform;
            transform.position = Vector3.zero;
            transform.rotation = Quaternion.identity;
            transform.localScale = Vector3.one;

            instance.State = PooledState.Pooled;
            activeCount--;

            Queue<CYGPooledInstance> queue = GetOrCreateQueue(instance.PrefabKey);
            if (queue.Count < MaxPerPrefab)
            {
                queue.Enqueue(instance);
                pooledCount++;
            }
            else
            {
                // Retention cap reached: destroy rather than keep growing the queue.
                UnityEngine.Object.Destroy(gameObject);
            }

            return true;
        }

        static CYGPooledInstance DequeueValid(Queue<CYGPooledInstance> queue)
        {
            while (queue.Count > 0)
            {
                CYGPooledInstance instance = queue.Dequeue();
                // A pooled GameObject can be destroyed from elsewhere; Unity's fake-null == is the detector.
                if (instance == null || instance.gameObject == null)
                {
                    pooledCount--;
                    continue;
                }

                return instance;
            }

            return null;
        }

        static CYGView EnsureView(CYGPooledInstance instance, GameObject go)
        {
            CYGView view = instance.CachedView;
            if (view == null)
            {
                view = go.GetComponent<CYGView>();
                if (view == null)
                {
                    view = go.AddComponent<CYGView>();
                }

                instance.CachedView = view;
            }

            return view;
        }

        static GameObject ResolvePrefab(string prefabName)
        {
            if (string.IsNullOrEmpty(prefabName))
            {
                Debug.LogError("[CYG] CYGObjectPool requires a prefab name.");
                return null;
            }

            CYGPrefabRegistry registry = CYGNetwork.CurrentPrefabs;
            if (registry == null)
            {
                Debug.LogError("[CYG] CYGObjectPool needs a CYGPrefabRegistry: assign one on CYGNetworkManager.");
                return null;
            }

            ushort prefabId = registry.GetPrefabId(prefabName);
            if (prefabId == ushort.MaxValue)
            {
                return null;
            }

            return registry.GetPrefab(prefabId);
        }

        static Queue<CYGPooledInstance> GetOrCreateQueue(string key)
        {
            Queue<CYGPooledInstance> queue;
            if (!pools.TryGetValue(key, out queue))
            {
                queue = new Queue<CYGPooledInstance>();
                pools[key] = queue;
            }

            return queue;
        }

        static Transform EnsureRoot()
        {
            if (poolRoot != null)
            {
                return poolRoot.transform;
            }

            poolRoot = new GameObject("[CYGObjectPool]");
            poolRoot.hideFlags = HideFlags.None;
            UnityEngine.Object.DontDestroyOnLoad(poolRoot);
            return poolRoot.transform;
        }

        static void EnsureSubscribed()
        {
            if (subscribed)
            {
                return;
            }

            subscribed = true;
            SceneManager.activeSceneChanged -= HandleActiveSceneChanged;
            SceneManager.activeSceneChanged += HandleActiveSceneChanged;
        }

        static void HandleActiveSceneChanged(Scene previousScene, Scene newScene)
        {
            // A scene swap invalidates gameplay objects; drop cached instances so the next spawn re-instantiates.
            Clear();
        }
    }

    /// <summary>Internal bookkeeping component attached to every pooled GameObject so despawn state and identity survive without trusting the dictionaries alone.</summary>
    sealed class CYGPooledInstance : MonoBehaviour
    {
        /// <summary>Prefab-name key of the queue this instance belongs to.</summary>
        public string PrefabKey;

        /// <summary>Whether the instance is currently handed out or sitting idle in its queue.</summary>
        public CYGObjectPool.PooledState State;

        /// <summary>Cached CYGView so despawn fires hooks without a GetComponent on the hot path (may be null).</summary>
        public CYGView CachedView;
    }
}
