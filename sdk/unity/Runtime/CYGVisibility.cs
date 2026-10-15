using System;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame
{
    /// <summary>Decides whether an observer may see a networked view.</summary>
    public interface ICYGVisibilityRule
    {
        /// <summary>Returns true when the observer is allowed to see the target view.</summary>
        bool IsVisibleTo(GameObject observer, CYGView target);
    }

    /// <summary>Hides views beyond a radius, with a hysteresis band to stop boundary flicker.</summary>
    [Serializable]
    public sealed class CYGDistanceRule : ICYGVisibilityRule
    {
        /// <summary>Distance at which a hidden view becomes visible again.</summary>
        public float visibilityRadius = 50f;

        /// <summary>Extra distance an already visible view may travel before being culled.</summary>
        public float cullHysteresis = 5f;

        /// <summary>Evaluates the plain entry radius.</summary>
        public bool IsVisibleTo(GameObject observer, CYGView target)
        {
            return IsVisibleTo(observer, target, false);
        }

        /// <summary>Evaluates with hysteresis based on the view's current visibility.</summary>
        public bool IsVisibleTo(GameObject observer, CYGView target, bool currentlyVisible)
        {
            if (observer == null || target == null)
            {
                return true;
            }

            // WHY: exit radius is larger than the entry radius so a view jittering at the boundary
            // toggles renderer state no more than once per hysteresis band.
            float radius = currentlyVisible ? visibilityRadius + cullHysteresis : visibilityRadius;
            return Vector3.Distance(observer.transform.position, target.transform.position) <= radius;
        }
    }

    /// <summary>Hides views outside the observer's grid cell and its immediate neighbours.</summary>
    [Serializable]
    public sealed class CYGGridRule : ICYGVisibilityRule
    {
        /// <summary>World size of one grid cell.</summary>
        public float cellSize = 50f;

        /// <summary>True checks the 3x3x3 cell neighbourhood; false checks the flat 3x3 cell ring.</summary>
        public bool threeDimensional = true;

        /// <summary>Evaluates the cell neighbourhood test.</summary>
        public bool IsVisibleTo(GameObject observer, CYGView target)
        {
            if (observer == null || target == null || cellSize <= 0f)
            {
                return true;
            }

            Vector3Int a = CellOf(observer.transform.position);
            Vector3Int b = CellOf(target.transform.position);
            int dx = Math.Abs(a.x - b.x);
            int dy = Math.Abs(a.y - b.y);
            int dz = Math.Abs(a.z - b.z);
            if (threeDimensional)
            {
                return dx <= 1 && dy <= 1 && dz <= 1;
            }

            return dx <= 1 && dz <= 1;
        }

        Vector3Int CellOf(Vector3 position)
        {
            float inv = 1f / cellSize;
            return new Vector3Int(Mathf.FloorToInt(position.x * inv), Mathf.FloorToInt(position.y * inv), Mathf.FloorToInt(position.z * inv));
        }
    }

    /// <summary>Rule kinds available to a CYGVisibilityRuleSet.</summary>
    public enum CYGVisibilityRuleKind : byte
    {
        /// <summary>Radius culling with hysteresis.</summary>
        Distance = 0,
        /// <summary>Cell neighbourhood culling.</summary>
        Grid = 1,
    }

    /// <summary>Serialized parameters for one rule inside a rule set.</summary>
    [Serializable]
    public sealed class CYGVisibilityRuleData
    {
        /// <summary>Which rule these parameters configure.</summary>
        public CYGVisibilityRuleKind kind = CYGVisibilityRuleKind.Distance;
        /// <summary>Distance rule entry radius.</summary>
        public float visibilityRadius = 50f;
        /// <summary>Distance rule cull hysteresis.</summary>
        public float cullHysteresis = 5f;
        /// <summary>Grid rule cell size.</summary>
        public float cellSize = 50f;
        /// <summary>Grid rule 3D neighbourhood toggle.</summary>
        public bool threeDimensional = true;

        /// <summary>Materializes the configured rule.</summary>
        public ICYGVisibilityRule CreateRule()
        {
            if (kind == CYGVisibilityRuleKind.Distance)
            {
                return new CYGDistanceRule { visibilityRadius = visibilityRadius, cullHysteresis = cullHysteresis };
            }

            return new CYGGridRule { cellSize = cellSize, threeDimensional = threeDimensional };
        }
    }

    /// <summary>Shared asset combining several visibility rules with AND semantics.</summary>
    public sealed class CYGVisibilityRuleSet : ScriptableObject
    {
        [SerializeField] List<CYGVisibilityRuleData> rules = new List<CYGVisibilityRuleData>();

        readonly List<ICYGVisibilityRule> compiled = new List<ICYGVisibilityRule>();
        int compiledCount = -1;

        /// <summary>Empty rule sets keep everything visible.</summary>
        public bool IsVisibleTo(GameObject observer, CYGView target)
        {
            return Evaluate(observer, target, true);
        }

        void OnValidate()
        {
            compiledCount = -1;
        }

        /// <summary>Runs every enabled rule; the view's current visibility feeds distance hysteresis.</summary>
        public bool Evaluate(GameObject observer, CYGView target, bool currentlyVisible)
        {
            if (rules == null || rules.Count == 0)
            {
                return true;
            }

            RecompileIfNeeded();

            for (int i = 0; i < compiled.Count; i++)
            {
                if (compiled[i] is CYGDistanceRule distance)
                {
                    if (!distance.IsVisibleTo(observer, target, currentlyVisible))
                    {
                        return false;
                    }
                }
                else if (!compiled[i].IsVisibleTo(observer, target))
                {
                    return false;
                }
            }

            return true;
        }

        void RecompileIfNeeded()
        {
            if (compiledCount == rules.Count)
            {
                return;
            }

            compiled.Clear();
            for (int i = 0; i < rules.Count; i++)
            {
                compiled.Add(rules[i].CreateRule());
            }

            compiledCount = rules.Count;
        }
    }

    /// <summary>Gates rendering and smoothing of a networked view for the local observer.</summary>
    public sealed class CYGVisibility : MonoBehaviour
    {
        [SerializeField] CYGVisibilityRuleSet ruleSet;
        [SerializeField] Transform observerOverride;
        [SerializeField] float evaluationInterval = 0.25f;

        readonly List<Renderer> renderers = new List<Renderer>();
        CYGView view;
        Behaviour smoothing;
        bool visible = true;
        float accumulator;

        /// <summary>True while this view passes the rule set for the local observer.</summary>
        public bool IsVisible => visible;

        void Awake()
        {
            view = GetComponent<CYGView>();
            GetComponentsInChildren<Renderer>(true, renderers);
            smoothing = GetComponent("CYGTransformSync") as Behaviour;
            CYGVisibilityFilter.Register(view);
        }

        void OnDestroy()
        {
            CYGVisibilityFilter.Unregister(view);
        }

        void Update()
        {
            accumulator += Time.deltaTime;
            if (accumulator < evaluationInterval)
            {
                return;
            }

            accumulator = 0f;
            EvaluateNow();
        }

        void EvaluateNow()
        {
            if (ruleSet == null || view == null)
            {
                SetVisible(true);
                return;
            }

            Transform observer = ResolveObserver();
            if (observer == null)
            {
                SetVisible(true);
                return;
            }

            SetVisible(ruleSet.Evaluate(observer.gameObject, view, visible));
        }

        // WHY: CYGPlayer exposes no transform, so the local camera (or an explicit override) is
        // the observer origin; unknown observers fail open rather than hiding the world.
        Transform ResolveObserver()
        {
            if (observerOverride != null)
            {
                return observerOverride;
            }

            return Camera.main != null ? Camera.main.transform : null;
        }

        void SetVisible(bool newState)
        {
            if (newState == visible)
            {
                return;
            }

            visible = newState;
            for (int i = 0; i < renderers.Count; i++)
            {
                renderers[i].enabled = newState;
            }

            if (smoothing != null)
            {
                smoothing.enabled = newState;
            }

            // WHY: CYGView exposes no visibility hook, so gated views record their state in the
            // filter; delta consumers can query ShouldReceive, and true network culling stays server-side.
            if (view != null)
            {
                CYGVisibilityFilter.SetGated(view.ViewId, !newState);
            }
        }
    }

    /// <summary>Per-view receive gate consulted when a view is culled by CYGVisibility.</summary>
    public static class CYGVisibilityFilter
    {
        static readonly Dictionary<int, bool> gatedByViewId = new Dictionary<int, bool>();

        /// <summary>Marks a view as registered with the filter.</summary>
        public static void Register(CYGView view)
        {
            // Views still unassigned at Awake gate nothing; their id arrives after this call.
            if (view != null && view.ViewId > 0)
            {
                gatedByViewId[view.ViewId] = false;
            }
        }

        /// <summary>Removes a view from the filter.</summary>
        public static void Unregister(CYGView view)
        {
            if (view != null && view.ViewId > 0)
            {
                gatedByViewId.Remove(view.ViewId);
            }
        }

        /// <summary>Sets whether a view's updates are currently dropped by the local client.</summary>
        public static void SetGated(int viewId, bool gated)
        {
            // WHY: assignment, not an update-if-registered, because a view's id can postdate its Awake.
            gatedByViewId[viewId] = gated;
        }

        /// <summary>False for culled views so listeners can skip delta processing.</summary>
        public static bool ShouldReceive(int viewId)
        {
            return !gatedByViewId.TryGetValue(viewId, out bool gated) || !gated;
        }
    }
}
