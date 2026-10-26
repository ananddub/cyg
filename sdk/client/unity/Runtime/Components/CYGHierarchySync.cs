using UnityEngine;

namespace CookYourGame
{
    /// <summary>
    /// Syncs a child GameObject's parent attachment and local pose so mounts (weapons in hands,
    /// a driver in a vehicle, gear on a rig) reproduce on every peer. The authority samples the
    /// transform in LateUpdate and rides the view's StateDelta channel as an observed component;
    /// no new opcode is introduced.
    /// Component blob layout (no length prefix; the receiver walks it purely by the flag byte so
    /// peers with different inspector settings still decode the exact same bytes):
    ///   [byte flags]  bit0 hasParent, bit1 hasScale, bit2 hasSibling
    ///   [uint varint parentViewId]  only when bit0 is set (the nearest CYGView above the parent)
    ///   [Vector3Packed localPosition]  6 bytes, always
    ///   [Quaternion localRotation]     4 bytes, always
    ///   [Vector3 localScale]           12 bytes, only when bit1 is set
    ///   [byte siblingIndex]            1 byte,  only when bit2 is set
    /// With a stable attachment the pose sits under the epsilons and nothing is transmitted.
    /// </summary>
    [DisallowMultipleComponent]
    [RequireComponent(typeof(CYGView))]
    [DefaultExecutionOrder(100)]
    [AddComponentMenu("CookYourGame/CYG Hierarchy Sync")]
    public class CYGHierarchySync : MonoBehaviour, ICYGObservable
    {
        private const byte FlagHasParent = 1;
        private const byte FlagHasScale = 2;
        private const byte FlagHasSibling = 4;

        // WHY: bounded retries cover async prefab-spawn ordering (parent view not registered yet)
        // without looping forever when the parent was never spawned for this peer.
        private const int MaxPendingAttempts = 60;

        [Tooltip("Transmit localScale alongside the pose.")]
        [SerializeField] private bool syncScale = true;

        [Tooltip("Transmit the child index within the parent so ordering matches on every peer.")]
        [SerializeField] private bool syncSiblingIndex = true;

        [Tooltip("Local position delta (world units) that marks the pose as changed.")]
        [SerializeField] private float positionThreshold = 0.0001f;

        [Tooltip("Local rotation delta (degrees) that marks the pose as changed.")]
        [SerializeField] private float rotationThreshold = 0.05f;

        private CYGView _view;
        private Transform _transform;

        // Authority sampling state.
        private bool _dirty;
        private bool _hasBaseline;
        private bool _hadAuthority;
        private CYGView _lastParentView;
        private Vector3 _lastPosition;
        private Quaternion _lastRotation;

        // Outgoing snapshot published by Serialize (values captured in LateUpdate).
        private bool _outHasParent;
        private int _outParentViewId;
        private bool _outHasScale;
        private bool _outHasSibling;
        private Vector3 _outPosition;
        private Quaternion _outRotation;
        private Vector3 _outScale;
        private int _outSibling;

        // Proxy state: a pending patch whose parent view is not resolvable yet.
        private bool _hasPending;
        private bool _pendingHasParent;
        private int _pendingParentViewId;
        private bool _pendingHasScale;
        private bool _pendingHasSibling;
        private Vector3 _pendingPosition;
        private Quaternion _pendingRotation;
        private Vector3 _pendingScale;
        private int _pendingSibling;
        private int _pendingAttempts;
        private bool _pendingGaveUpWarned;
        private bool _warnedNoView;

        private void Awake()
        {
            _transform = transform;
        }

        private void OnEnable()
        {
            if (_transform == null) _transform = transform;

            _view = GetComponent<CYGView>();
            if (_view == null)
            {
                if (!_warnedNoView)
                {
                    _warnedNoView = true;
                    Debug.LogWarning(GetType().Name + " on " + gameObject.name +
                                     " has no CYGView on the same GameObject; hierarchy will not sync.");
                }
                return;
            }
            _view.AddObservable(this);

            CYGNetwork.OnObjectDespawned += HandleDespawned;
        }

        private void OnDisable()
        {
            if (_view != null) _view.RemoveObservable(this);
            CYGNetwork.OnObjectDespawned -= HandleDespawned;
            _hasPending = false;
        }

        private void OnDestroy()
        {
            CYGNetwork.OnObjectDespawned -= HandleDespawned;
        }

        // WHY: sample on LateUpdate so the pose is read after gameplay and any CYGTransformSync have
        // finished writing the transform this frame. Only the authority publishes.
        private void LateUpdate()
        {
            if (_transform == null) return;

            bool authority = HasAuthority;
            if (authority != _hadAuthority)
            {
                // Ownership flipped: drop any stale baseline so the new authority re-publishes the
                // attachment once, and forget a queued proxy patch that can no longer be trusted.
                _hadAuthority = authority;
                _hasBaseline = false;
                _dirty = false;
                if (authority)
                {
                    _hasPending = false;
                    _pendingGaveUpWarned = false;
                }
            }

            if (authority)
            {
                SampleAuthority();
                return;
            }

            if (_hasPending) RetryPending();
        }

        private void SampleAuthority()
        {
            CYGView parentView = ResolveParentView(_transform.parent);
            int parentViewId = parentView != null ? parentView.ViewId : 0;
            bool hasParent = parentViewId > 0;

            Vector3 position = _transform.localPosition;
            Quaternion rotation = _transform.localRotation;
            Vector3 scale = _transform.localScale;
            int sibling = _transform.GetSiblingIndex();

            if (!_hasBaseline)
            {
                CommitBaseline(parentView, position, rotation);
                Publish(hasParent, parentViewId, position, rotation, scale, sibling);
                _dirty = true;
                return;
            }

            bool parentChanged = !ReferenceEquals(parentView, _lastParentView);
            float posDeltaSqr = (position - _lastPosition).sqrMagnitude;
            float thresholdSqr = positionThreshold * positionThreshold;
            bool poseMoved = posDeltaSqr > thresholdSqr ||
                             Quaternion.Angle(_lastRotation, rotation) > rotationThreshold;

            if (parentChanged || poseMoved)
            {
                Publish(hasParent, parentViewId, position, rotation, scale, sibling);
                CommitBaseline(parentView, position, rotation);
                _dirty = true;
            }
        }

        private void Publish(bool hasParent, int parentViewId, Vector3 position, Quaternion rotation,
                             Vector3 scale, int sibling)
        {
            _outHasParent = hasParent;
            _outParentViewId = parentViewId;
            _outHasScale = syncScale;
            _outHasSibling = syncSiblingIndex;
            _outPosition = position;
            _outRotation = rotation;
            _outScale = scale;
            _outSibling = sibling;
        }

        private void CommitBaseline(CYGView parentView, Vector3 position, Quaternion rotation)
        {
            _lastParentView = parentView;
            _lastPosition = position;
            _lastRotation = rotation;
            _hasBaseline = true;
        }

        private static CYGView ResolveParentView(Transform parent)
        {
            // Nearest CYGView at or above the direct parent; a non-networked parent subtree resolves
            // to null and is reported as "world root" because there is no attachment id to publish.
            if (parent == null) return null;
            CYGView view = parent.GetComponentInParent<CYGView>();
            if (view != null && view.ViewId <= 0) return null;
            return view;
        }

        private bool HasAuthority
        {
            get { return _view != null && _view.HasAuthority; }
        }

        /// <summary>True when the authority's attachment or pose changed since the last pack.</summary>
        public bool HasDelta()
        {
            return _dirty;
        }

        /// <summary>Writes the attachment snapshot described in the class summary and clears dirty.</summary>
        public void Serialize(CYGBitWriter writer)
        {
            byte flags = 0;
            if (_outHasParent) flags |= FlagHasParent;
            if (_outHasScale) flags |= FlagHasScale;
            if (_outHasSibling) flags |= FlagHasSibling;

            writer.WriteByte(flags);
            if (_outHasParent) writer.WriteUInt32((uint)_outParentViewId);
            writer.WriteVector3Packed(_outPosition);
            writer.WriteQuaternion(_outRotation);
            if (_outHasScale) writer.WriteVector3(_outScale);
            if (_outHasSibling) writer.WriteByte((byte)Mathf.Clamp(_outSibling, 0, byte.MaxValue));

            _dirty = false;
        }

        /// <summary>Consumes exactly the bytes Serialize wrote, then applies or queues the patch.</summary>
        public void Deserialize(CYGBitReader reader)
        {
            byte flags = reader.ReadByte();
            bool hasParent = (flags & FlagHasParent) != 0;
            bool hasScale = (flags & FlagHasScale) != 0;
            bool hasSibling = (flags & FlagHasSibling) != 0;

            int parentViewId = hasParent ? (int)reader.ReadUInt32() : 0;
            Vector3 position = reader.ReadVector3Packed();
            Quaternion rotation = reader.ReadQuaternion();
            Vector3 scale = hasScale ? reader.ReadVector3() : Vector3.zero;
            int sibling = hasSibling ? reader.ReadByte() : 0;

            // Authority decodes to keep the stream aligned but never overwrites its own transform.
            if (HasAuthority) return;

            ApplyOrQueue(hasParent, parentViewId, hasScale, hasSibling, position, rotation, scale, sibling);
        }

        private void ApplyOrQueue(bool hasParent, int parentViewId, bool hasScale, bool hasSibling,
                                  Vector3 position, Quaternion rotation, Vector3 scale, int sibling)
        {
            Transform newParent = null;
            if (hasParent)
            {
                CYGView parentView;
                if (!CYGViewTable.TryGet(parentViewId, out parentView) || parentView == null)
                {
                    QueuePending(hasParent, parentViewId, hasScale, hasSibling, position, rotation, scale, sibling);
                    return;
                }
                newParent = parentView.transform;
            }

            if (!ApplyAttachment(newParent, hasScale, hasSibling, position, rotation, scale, sibling))
            {
                // ApplyAttachment dropped an invalid (self/descendant) parent; nothing further to do.
                return;
            }

            _hasPending = false;
            _pendingGaveUpWarned = false;
            _pendingAttempts = 0;
        }

        private void QueuePending(bool hasParent, int parentViewId, bool hasScale, bool hasSibling,
                                  Vector3 position, Quaternion rotation, Vector3 scale, int sibling)
        {
            _pendingHasParent = hasParent;
            _pendingParentViewId = parentViewId;
            _pendingHasScale = hasScale;
            _pendingHasSibling = hasSibling;
            _pendingPosition = position;
            _pendingRotation = rotation;
            _pendingScale = scale;
            _pendingSibling = sibling;
            _pendingAttempts = 0;
            _pendingGaveUpWarned = false;
            _hasPending = true;
        }

        private void RetryPending()
        {
            Transform newParent = null;
            if (_pendingHasParent)
            {
                CYGView parentView;
                if (!CYGViewTable.TryGet(_pendingParentViewId, out parentView) || parentView == null)
                {
                    if (_pendingAttempts < MaxPendingAttempts)
                    {
                        _pendingAttempts++;
                        return;
                    }
                    if (!_pendingGaveUpWarned)
                    {
                        _pendingGaveUpWarned = true;
                        Debug.LogWarning(GetType().Name + " on " + gameObject.name + ": could not resolve parent view " +
                                         _pendingParentViewId + " within " + MaxPendingAttempts +
                                         " retries; dropping the hierarchy patch.");
                    }
                    _hasPending = false;
                    return;
                }
                newParent = parentView.transform;
            }

            if (ApplyAttachment(newParent, _pendingHasScale, _pendingHasSibling, _pendingPosition,
                                _pendingRotation, _pendingScale, _pendingSibling))
            {
                _hasPending = false;
                _pendingGaveUpWarned = false;
                _pendingAttempts = 0;
            }
        }

        private bool ApplyAttachment(Transform newParent, bool hasScale, bool hasSibling,
                                     Vector3 position, Quaternion rotation, Vector3 scale, int sibling)
        {
            if (_transform == null) _transform = transform;
            if (_transform == null) return false;

            if (newParent != null)
            {
                // Reparenting into self or into a descendant of self would corrupt the Unity hierarchy.
                if (newParent == _transform || _transform.IsChildOf(newParent))
                {
                    Debug.LogError(GetType().Name + " on " + gameObject.name +
                                   ": refusing to parent under self or a descendant of self; patch dropped.");
                    _hasPending = false;
                    return false;
                }
                _transform.SetParent(newParent, false);
            }
            else
            {
                _transform.SetParent(null, false);
            }

            _transform.localPosition = position;
            _transform.localRotation = rotation;
            if (hasScale) _transform.localScale = scale;
            if (hasSibling) _transform.SetSiblingIndex(sibling);
            return true;
        }

        // A parent we are waiting on may be despawned instead of never arriving: discard the patch
        // rather than burning the retry budget, and stay quiet (no warning) because this is expected.
        private void HandleDespawned(int viewId)
        {
            if (_hasPending && _pendingParentViewId == viewId) _hasPending = false;
        }

        /// <summary>Resets sampling so the next authority frame re-publishes the current attachment.</summary>
        public void ResetAuthorityBaseline()
        {
            _hasBaseline = false;
            _dirty = false;
            _lastParentView = null;
        }
    }
}
