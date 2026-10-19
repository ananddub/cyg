using UnityEngine;

namespace CookYourGame
{
    /// <summary>
    /// Purely-local ownership switch: enables the "owner" set when the view belongs to the local
    /// player (view.IsMine) and the "proxy" set otherwise. It sends nothing and costs zero wire
    /// bytes: it derives from CYGBehaviour only to inherit the spawn/owner-change callbacks that
    /// fire via CYGView.InvokeOwnerChangedHooks, never registers itself as observed, and reports
    /// HasDelta() as false so it is never walked by the dirty mask.
    /// </summary>
    [AddComponentMenu("CookYourGame/CYG Ownership Toggle")]
    public class CYGOwnershipToggle : CYGBehaviour
    {
        [Tooltip("Behaviours enabled only for the local owner (camera, input, local effects).")]
        [SerializeField] private Behaviour[] enableWhenOwner;

        [Tooltip("Behaviours enabled only for non-owners (remote proxies, e.g. a hitbox collider).")]
        [SerializeField] private Behaviour[] disableWhenOwner;

        private bool _warnedNoView;

        private void Awake()
        {
            Apply();
        }

        protected override void OnSpawned()
        {
            Apply();
        }

        protected override void OnOwnerChanged(CYGPlayer newOwner)
        {
            Apply();
        }

        /// <summary>Re-evaluates the two lists against the current ownership; safe to call any time.</summary>
        public void Apply()
        {
            CYGView view = View;
            if (view == null)
            {
                if (!_warnedNoView)
                {
                    _warnedNoView = true;
                    Debug.LogWarning(GetType().Name + " on " + gameObject.name +
                                     " has no CYGView in its parent chain; ownership toggle is a no-op.");
                }
                return;
            }

            bool mine = view.IsMine;
            SetGroup(enableWhenOwner, mine);
            SetGroup(disableWhenOwner, !mine);
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
