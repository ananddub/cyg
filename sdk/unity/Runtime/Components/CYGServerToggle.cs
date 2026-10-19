using UnityEngine;

namespace CookYourGame
{
    /// <summary>
    /// Purely-local authority switch: enables the "authority" set while this peer holds room
    /// authority (CYGNetwork.IsMasterClient) and the "non-authority" set otherwise. When
    /// respectSceneObjectAuthority is on, a peer that owns a specific view (or a scene object it
    /// drives) counts as authoritative for that object only. It sends nothing: like
    /// CYGOwnershipToggle it derives from CYGBehaviour solely to inherit lifecycle callbacks,
    /// never registers as observed, and reports HasDelta() false so it costs zero wire bytes.
    /// Master-client status has no CYGNetwork event, so the flag is polled in Update behind a
    /// cheap change check; the connect/join events only force an early re-evaluation.
    /// </summary>
    [AddComponentMenu("CookYourGame/CYG Server Toggle")]
    public class CYGServerToggle : CYGBehaviour
    {
        [Tooltip("Behaviours enabled only on the authoritative peer (spawners, simulation, hitscan).")]
        [SerializeField] private Behaviour[] enableWhenAuthority;

        [Tooltip("Behaviours enabled only on non-authoritative peers (client-side prediction stand-ins).")]
        [SerializeField] private Behaviour[] disableWhenAuthority;

        [Tooltip("Also treat per-view authority (owning this view) as authoritative, not just the master client.")]
        [SerializeField] private bool respectSceneObjectAuthority;

        private bool _lastRole;
        private bool _subscribed;

        private void Awake()
        {
            Apply();
        }

        private void OnEnable()
        {
            if (!_subscribed)
            {
                CYGNetwork.OnConnectedEvent += HandleConnected;
                CYGNetwork.OnJoinedRoom += HandleJoinedRoom;
                _subscribed = true;
            }
            Apply();
        }

        private void OnDisable()
        {
            if (_subscribed)
            {
                CYGNetwork.OnConnectedEvent -= HandleConnected;
                CYGNetwork.OnJoinedRoom -= HandleJoinedRoom;
                _subscribed = false;
            }
        }

        private void OnDestroy()
        {
            if (_subscribed)
            {
                CYGNetwork.OnConnectedEvent -= HandleConnected;
                CYGNetwork.OnJoinedRoom -= HandleJoinedRoom;
                _subscribed = false;
            }
        }

        protected override void OnSpawned()
        {
            Apply();
        }

        protected override void OnOwnerChanged(CYGPlayer newOwner)
        {
            Apply();
        }

        // WHY: IsMasterClient flips when the lowest-actor owner changes (roster events) and CYGNetwork
        // publishes no event for it, so poll the bool behind a change check instead of per-frame writes.
        private void Update()
        {
            if (HasServerRole() != _lastRole) Apply();
        }

        /// <summary>Re-evaluates both lists against the current authority; safe to call any time.</summary>
        public void Apply()
        {
            bool authority = HasServerRole();
            _lastRole = authority;
            SetGroup(enableWhenAuthority, authority);
            SetGroup(disableWhenAuthority, !authority);
        }

        private bool HasServerRole()
        {
            if (CYGNetwork.IsMasterClient) return true;
            if (respectSceneObjectAuthority)
            {
                CYGView view = View;
                if (view != null && view.HasAuthority) return true;
            }
            return false;
        }

        private static void SetGroup(Behaviour[] group, bool enabled)
        {
            if (group == null) return;
            for (int i = 0; i < group.Length; i++)
            {
                Behaviour behaviour = group[i];
                if (behaviour == null) continue;
                behaviour.enabled = enabled;
            }
        }

        private void HandleConnected()
        {
            Apply();
        }

        private void HandleJoinedRoom(string roomName)
        {
            Apply();
        }

        /// <inheritdoc/>
        public override bool HasDelta()
        {
            return false;
        }

        /// <inheritdoc/>
        public override void Serialize(CYGBitWriter writer)
        {
            // No wire state; kept as an explicit no-op so an accidental registration is byte-symmetric.
        }

        /// <inheritdoc/>
        public override void Deserialize(CYGBitReader reader)
        {
            // No wire state; pairs with the no-op Serialize above.
        }
    }
}
