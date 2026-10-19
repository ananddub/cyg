using System;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame
{
    /// <summary>A component whose state is streamed as part of a CYGView delta.</summary>
    public interface ICYGObservable
    {
        /// <summary>True when local state changed since the last pack.</summary>
        bool HasDelta();
        /// <summary>Writes the delta payload; must not mutate wire-visible state after Deserialize expectation.</summary>
        void Serialize(CYGBitWriter writer);
        /// <summary>Consumes exactly the bytes Serialize produced, in the same order.</summary>
        void Deserialize(CYGBitReader reader);
    }

    /// <summary>Global viewId to CYGView lookup used by the sync pump and delta dispatch.</summary>
    public static class CYGViewTable
    {
        private static readonly Dictionary<int, CYGView> s_Views = new Dictionary<int, CYGView>(256);

        /// <summary>Maps a view by its ViewId; ignores invalid ids.</summary>
        public static void Register(CYGView view)
        {
            if (view == null || view.ViewId <= 0) return;
            s_Views[view.ViewId] = view;
        }

        /// <summary>Removes the mapping for a view id.</summary>
        public static void Unregister(int viewId)
        {
            if (viewId <= 0) return;
            s_Views.Remove(viewId);
        }

        /// <summary>Finds a live view by id.</summary>
        public static bool TryGet(int viewId, out CYGView view)
        {
            return s_Views.TryGetValue(viewId, out view);
        }

        /// <summary>Drops every mapping (session teardown).</summary>
        public static void Clear()
        {
            s_Views.Clear();
        }

        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.SubsystemRegistration)]
        static void ResetDomain()
        {
            Clear();
        }

        /// <summary>All registered views.</summary>
        public static IEnumerable<CYGView> All
        {
            get { return s_Views.Values; }
        }
    }

    /// <summary>
    /// Identity + ownership for a networked GameObject and the container for its observed components.
    /// Per-view delta wire format (payload of SendViewDelta):
    ///   [byte dirtyMask]  bit i set = observedComponents[i] has a delta (only the first 8 components are addressable)
    ///   then, for each set bit in ascending index order, that component's Serialize() blob with no length prefix;
    ///   the reader walks the same ascending order so blob boundaries are implicit per component.
    /// </summary>
    [DisallowMultipleComponent]
    [DefaultExecutionOrder(-500)]
    public class CYGView : MonoBehaviour
    {
        /// <summary>Max observed components that fit the dirty bitmask (one byte).</summary>
        public const int MaxObservedComponents = 8;

        [SerializeField] private int _viewId;
        private int _ownerActor = -1;

        /// <summary>Network-wide unique view id (0 until assigned).</summary>
        public int ViewId
        {
            get { return _viewId; }
        }

        /// <summary>Actor number owning input/authority over this view (0 = scene object, -1 = unassigned).</summary>
        public int OwnerActor
        {
            get { return _ownerActor; }
        }

        /// <summary>True when the local player owns this view.</summary>
        public bool IsMine
        {
            get { return CYGNetwork.LocalPlayer != null && _ownerActor == CYGNetwork.LocalPlayer.ActorNumber; }
        }

        /// <summary>True when the view was placed in the scene and has no player owner.</summary>
        public bool IsSceneObject
        {
            get { return _ownerActor == 0; }
        }

        /// <summary>True when the local client may write state for this view.</summary>
        public bool HasAuthority
        {
            get { return IsMine || (IsSceneObject && CYGNetwork.IsMasterClient); }
        }

        /// <summary>Components streamed with this view's deltas (index order is the wire order).</summary>
        public List<ICYGObservable> observedComponents = new List<ICYGObservable>();

        private void Awake()
        {
            if (_viewId > 0) CYGViewTable.Register(this);
        }

        /// <summary>Assigns identity from core spawn logic and registers the view.</summary>
        public void AssignViewId(int viewId, int ownerActor)
        {
            if (viewId <= 0)
            {
                Debug.LogError("CYGView.AssignViewId: invalid viewId " + viewId + " on " + gameObject.name);
                return;
            }
            if (_viewId > 0 && _viewId != viewId) CYGViewTable.Unregister(_viewId);
            _viewId = viewId;
            _ownerActor = ownerActor;
            CYGViewTable.Register(this);
        }

        /// <summary>Adds an observable if not present; refuses to exceed the 8-bit dirty mask.</summary>
        public void AddObservable(ICYGObservable component)
        {
            if (component == null || observedComponents.Contains(component)) return;
            if (observedComponents.Count >= MaxObservedComponents)
            {
                Debug.LogWarning("CYGView on " + gameObject.name + ": only " + MaxObservedComponents +
                                 " observed components are supported (dirty bitmask limit), ignored " + component);
                return;
            }
            observedComponents.Add(component);
        }

        /// <summary>Removes an observable from the delta pump.</summary>
        public void RemoveObservable(ICYGObservable component)
        {
            if (component != null) observedComponents.Remove(component);
        }

        /// <summary>Fires OnSpawned on every CYGBehaviour on this GameObject and on the view itself.</summary>
        public void InvokeSpawnHooks()
        {
            var behaviours = GetComponents<CYGBehaviour>();
            for (int i = 0; i < behaviours.Length; i++) behaviours[i].InvokeSpawned();
            OnSpawned();
        }

        /// <summary>Fires OnDespawned hooks and removes this view from the table.</summary>
        public void InvokeDespawnHooks()
        {
            var behaviours = GetComponents<CYGBehaviour>();
            for (int i = 0; i < behaviours.Length; i++) behaviours[i].InvokeDespawned();
            OnDespawned();
            CYGViewTable.Unregister(_viewId);
        }

        /// <summary>Fires OnOwnerChanged on behaviours and the view after an ownership transfer.</summary>
        public void InvokeOwnerChangedHooks(CYGPlayer newOwner)
        {
            _ownerActor = newOwner != null ? newOwner.ActorNumber : -1;
            var behaviours = GetComponents<CYGBehaviour>();
            for (int i = 0; i < behaviours.Length; i++) behaviours[i].InvokeOwnerChanged(newOwner);
            OnOwnerChanged(newOwner);
        }

        /// <summary>Called after this view is spawned (override point).</summary>
        public virtual void OnSpawned() { }

        /// <summary>Called before this view is despawned (override point).</summary>
        public virtual void OnDespawned() { }

        /// <summary>Called when view ownership changed (override point).</summary>
        public virtual void OnOwnerChanged(CYGPlayer newOwner) { }

        /// <summary>True when at least one of the first 8 observed components reports a delta.</summary>
        public bool HasAnyDelta()
        {
            int n = observedComponents.Count;
            if (n > MaxObservedComponents) n = MaxObservedComponents;
            for (int i = 0; i < n; i++)
            {
                ICYGObservable c = observedComponents[i];
                if (c != null && c.HasDelta()) return true;
            }
            return false;
        }

        /// <summary>Writes [dirtyMask byte] then each dirty component's blob in ascending index order.</summary>
        public void CollectDeltas(CYGBitWriter writer)
        {
            if (writer == null) return;
            int n = observedComponents.Count;
            if (n > MaxObservedComponents) n = MaxObservedComponents;

            byte mask = 0;
            for (int i = 0; i < n; i++)
            {
                ICYGObservable c = observedComponents[i];
                if (c != null && c.HasDelta()) mask |= (byte)(1 << i);
            }
            writer.WriteByte(mask);

            for (int i = 0; i < n; i++)
            {
                if ((mask & (1 << i)) == 0) continue;
                observedComponents[i].Serialize(writer);
            }
        }

        /// <summary>Reads the dirty mask and feeds each blob to its component in the same ascending order.</summary>
        public void ApplyDeltaStream(CYGBitReader reader)
        {
            if (reader == null) return;
            byte mask = reader.ReadByte();
            int n = observedComponents.Count;
            for (int i = 0; i < MaxObservedComponents; i++)
            {
                if ((mask & (1 << i)) == 0) continue;
                if (i >= n)
                {
                    // Components past the local list make the stream undecodable (async prefab load, version skew).
                    Debug.LogWarning("CYGView " + _viewId + ": delta references component " + i + " but only " + n + " are attached");
                    return;
                }
                ICYGObservable c = observedComponents[i];
                if (c != null) c.Deserialize(reader);
            }
        }
    }

    /// <summary>Owns the periodic send pump and inbound delta dispatch for all views.</summary>
    public static class CYGSyncDriver
    {
        private static readonly CYGBitWriter s_PumpWriter = new CYGBitWriter();
        private static readonly CYGBitReader s_PumpReader = new CYGBitReader();
        private static readonly List<CYGView> s_ScratchViews = new List<CYGView>(128);
        private static bool s_Booted;

        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.SubsystemRegistration)]
        static void ResetDomain()
        {
            s_Booted = false;
            s_ScratchViews.Clear();
        }

        /// <summary>Subscribes to the network tick and view delta events exactly once.</summary>
        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.BeforeSceneLoad)]
        private static void Boot()
        {
            if (s_Booted) return;
            s_Booted = true;
            CYGNetworkManager.OnFixedNetworkTick += Tick;
            CYGNetwork.OnViewDelta += OnViewDelta;
            CYGNetwork.OnDisconnectedEvent += OnDisconnected;
        }

        private static void Tick()
        {
            // Snapshot first: a despawn triggered inside SendViewDelta must not invalidate the iteration.
            s_ScratchViews.Clear();
            foreach (CYGView view in CYGViewTable.All)
            {
                if (view == null || !view.HasAuthority) continue;
                if (!view.HasAnyDelta()) continue;
                s_ScratchViews.Add(view);
            }

            for (int i = 0; i < s_ScratchViews.Count; i++)
            {
                CYGView view = s_ScratchViews[i];
                if (view == null || view.ViewId <= 0) continue;
                s_PumpWriter.Reset();
                view.CollectDeltas(s_PumpWriter);
                // Assumption: the core copies the segment into its own frame buffer before returning,
                // so the shared writer's memory can be reused on the next call.
                CYGNetwork.SendViewDelta(view.ViewId, s_PumpWriter.ToArraySegment());
            }
        }

        private static void OnViewDelta(int viewId, ArraySegment<byte> payload)
        {
            if (viewId <= 0 || payload.Array == null || payload.Count <= 0) return;
            if (!CYGViewTable.TryGet(viewId, out CYGView view) || view == null) return;
            // The payload buffer is owned by the transport and may be recycled the moment we return,
            // so the stream is decoded synchronously with no retained copy.
            s_PumpReader.Attach(payload.Array, payload.Offset, payload.Count);
            view.ApplyDeltaStream(s_PumpReader);
        }

        private static void OnDisconnected(string reason)
        {
            s_ScratchViews.Clear();
            CYGViewTable.Clear();
        }
    }
}
