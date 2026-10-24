using System;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Convenience base for every MonoBehaviour that talks to the simulation.
    /// Mirrors referenceMonoBehaviour: exposes the runner, session, world,
    /// current tick and local-player identity without each script resolving
    /// them on its own.
    /// </summary>
    public abstract class CYGMonoBehaviour : MonoBehaviour
    {
        [Header("Simulation")]
        [Tooltip("Optional explicit runner. When empty the active CYGDeterministicRunner.Instance is used.")]
        [SerializeField] private CYGDeterministicRunner runnerOverride;

        private CYGDeterministicRunner _cachedRunner;
        private readonly List<IDisposable> _subscriptions = new List<IDisposable>();

        /// <summary>
        /// The runner this behaviour is bound to, or null when no runner exists.
        /// This property never creates a runner; callers that require one must
        /// check <see cref="HasRunner"/> or use <see cref="TryGetRunner"/>.
        /// </summary>
        public CYGDeterministicRunner Runner
        {
            get
            {
                CYGDeterministicRunner runner;
                TryGetRunner(out runner);
                return runner;
            }
        }

        /// <summary>True when an explicit or active runner is available.</summary>
        public bool HasRunner
        {
            get
            {
                CYGDeterministicRunner runner;
                return TryGetRunner(out runner);
            }
        }

        /// <summary>
        /// Resolves the explicit override or the active runner without creating
        /// one implicitly. Implicit creation used to happen from view scripts,
        /// callbacks and updaters in menu/bootstrap scenes, which could start a
        /// second simulation or spawn unexpected scene objects.
        /// </summary>
        public bool TryGetRunner(out CYGDeterministicRunner runner)
        {
            if (runnerOverride != null)
            {
                runner = runnerOverride;
                return true;
            }

            if (_cachedRunner != null)
            {
                runner = _cachedRunner;
                return true;
            }

            runner = CYGDeterministicRunner.Instance;
            if (runner == null)
            {
#if UNITY_2023_1_OR_NEWER
                runner = UnityEngine.Object.FindAnyObjectByType<CYGDeterministicRunner>();
#else
                runner = UnityEngine.Object.FindObjectOfType<CYGDeterministicRunner>();
#endif
            }

            _cachedRunner = runner;
            return runner != null;
        }

        /// <summary>Explicitly binds this behaviour to a runner created by a bootstrap.</summary>
        public void SetRunner(CYGDeterministicRunner runner)
        {
            runnerOverride = runner;
            _cachedRunner = runner;
        }

        public CYGWorldRollbackSession Session
        {
            get
            {
                CYGDeterministicRunner runner;
                return TryGetRunner(out runner) ? runner.Session : null;
            }
        }

        public CYGWorld World => Session != null ? Session.World : null;
        public int CurrentTick => Session != null ? Session.CurrentTick : 0;
        public int ConfirmedTick => Session != null ? Session.ConfirmedTick : 0;

        /// <summary>Deterministic simulation fixed delta time (e.g. 1/60s).</summary>
        public FP DeltaTime => Session != null ? Session.DeltaTime : FP.FromFraction(1, 60);

        public bool IsSimulationRunning
        {
            get
            {
                CYGDeterministicRunner runner;
                return TryGetRunner(out runner)
                    && runner.IsSimulationStarted
                    && runner.Session != null;
            }
        }

        public int LocalPlayerId
        {
            get
            {
                CYGDeterministicRunner runner;
                return TryGetRunner(out runner) ? runner.LocalPlayerId : 0;
            }
        }

        public CYGPlayerRef LocalPlayerRef
        {
            get
            {
                int id = LocalPlayerId;
                return id > 0 ? new CYGPlayerRef((ushort)id) : CYGPlayerRef.None;
            }
        }

        public bool IsLocalPlayer => LocalPlayerId > 0;

        public CYGEntityViewUpdater EntityViewUpdater
        {
            get
            {
                CYGDeterministicRunner runner;
                return TryGetRunner(out runner) ? runner.EntityViews : null;
            }
        }

        public CYGViewContext ViewContext
        {
            get
            {
                CYGDeterministicRunner runner;
                return TryGetRunner(out runner) ? runner.ViewContext : null;
            }
        }

        /// <summary>
        /// Subscribes to a deterministic simulation event. Automatically unregistered
        /// on OnDestroy so components never keep dangling callbacks alive.
        /// </summary>
        protected IDisposable Subscribe<T>(Action<T> handler) where T : CYGEvent
        {
            IDisposable subscription = CYGEvents.Subscribe(this, handler);
            _subscriptions.Add(subscription);
            return subscription;
        }

        protected virtual void Reset()
        {
            runnerOverride = GetComponentInParent<CYGDeterministicRunner>();
        }

        protected virtual void OnDestroy()
        {
            for (int i = 0; i < _subscriptions.Count; i++)
            {
                _subscriptions[i]?.Dispose();
            }
            _subscriptions.Clear();
            _cachedRunner = null;
        }
    }
}
