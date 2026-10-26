using System;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Base class for scripts that hang under a <see cref="CYGEntityView"/>.
    /// The view drives the lifecycle:
    /// <see cref="OnInitialize"/> once when the component is first bound,
    /// <see cref="OnActivate()"/> when the entity view spawns or is reused from
    /// the pool, <see cref="OnDeactivate"/> before it is pooled or destroyed,
    /// <see cref="OnUpdateView"/> during the visual frame, and
    /// <see cref="OnLateUpdateView"/> after camera/transform work.
    ///
    /// Disabled components are not updated, matching Unity's
    /// IsActiveAndEnabled behaviour.
    /// </summary>
    public abstract class CYGEntityViewComponent : CYGMonoBehaviour
    {
        private readonly List<IDisposable> _subscriptions = new List<IDisposable>();
        private readonly List<IDisposable> _persistentSubscriptions = new List<IDisposable>();
        private bool _initializing;

        public CYGEntityView View { get; private set; }
        public CYGEntityRef Entity => View != null ? View.Entity : CYGEntityRef.None;
        public new CYGWorld World => View != null ? View.World : null;
        public new CYGViewContext ViewContext => View != null ? View.ViewContext : null;
        public bool HasView => View != null;
        public bool IsActive => View != null && View.IsActive;
        public bool IsActiveAndEnabled => IsActive && isActiveAndEnabled;
        public bool IsInitialized { get; private set; }
        public bool IsPredicted => View != null && View.IsPredicted;
        public bool IsVerified => View != null && View.IsVerified;
        public bool IsLocalPlayerView => View != null && View.IsLocalPlayer;

        protected virtual void OnInitialize() { }

        /// <summary>Legacy activate hook without explicit world/entity arguments.</summary>
        protected virtual void OnActivate() { }

        /// <summary>
        /// Activate hook with the deterministic entity context. Override this
        /// when you need the world/entity during activation; the default calls
        /// <see cref="OnActivate()"/> so existing scripts keep working.
        /// </summary>
        protected virtual void OnActivate(CYGWorld world, CYGEntityRef entity)
        {
            OnActivate();
        }

        protected virtual void OnDeactivate() { }
        protected virtual void OnUpdateView() { }
        protected virtual void OnLateUpdateView() { }

        /// <summary>Called when the observed simulation/world changes.</summary>
        protected virtual void OnGameChanged() { }

        /// <summary>Finds another view component of the given type on this entity view.</summary>
        protected T GetViewComponent<T>() where T : CYGEntityViewComponent
        {
            if (View == null)
            {
                return null;
            }

            CYGEntityViewComponent[] components = View.Components;
            for (int i = 0; i < components.Length; i++)
            {
                if (components[i] != null && components[i] is T typed)
                {
                    return typed;
                }
            }
            return null;
        }

        /// <summary>
        /// Typed event subscription that is released automatically when the
        /// view deactivates or the component is destroyed. Subscriptions made
        /// during <see cref="OnInitialize"/> survive pooling and are suppressed
        /// while the view is inactive.
        /// </summary>
        protected new IDisposable Subscribe<T>(Action<T> handler) where T : CYGEvent
        {
            if (_initializing)
            {
                IDisposable persistent = CYGEvents.Subscribe(this, (T value) =>
                {
                    if (IsActiveAndEnabled)
                    {
                        handler(value);
                    }
                });
                _persistentSubscriptions.Add(persistent);
                return persistent;
            }

            IDisposable subscription = CYGEvents.Subscribe(this, handler);
            _subscriptions.Add(subscription);
            return subscription;
        }

        internal void BindViewInternal(CYGEntityView view)
        {
            View = view;
            if (view != null && !IsInitialized)
            {
                IsInitialized = true;
                _initializing = true;
                try
                {
                    OnInitialize();
                }
                finally
                {
                    _initializing = false;
                }
            }
        }

        internal void ActivateInternal()
        {
            if (View == null)
            {
                return;
            }

            OnActivate(View.World, View.Entity);
        }

        internal void DeactivateInternal()
        {
            if (View == null)
            {
                return;
            }

            try
            {
                OnDeactivate();
            }
            finally
            {
                DisposeSubscriptions();
                View = null;
            }
        }

        internal void UpdateViewInternal()
        {
            if (IsActiveAndEnabled)
            {
                OnUpdateView();
            }
        }

        internal void LateUpdateViewInternal()
        {
            if (IsActiveAndEnabled)
            {
                OnLateUpdateView();
            }
        }

        internal void GameChangedInternal()
        {
            if (View != null)
            {
                OnGameChanged();
            }
        }

        private void DisposeSubscriptions()
        {
            for (int i = 0; i < _subscriptions.Count; i++)
            {
                _subscriptions[i]?.Dispose();
            }
            _subscriptions.Clear();
        }

        private void DisposePersistentSubscriptions()
        {
            for (int i = 0; i < _persistentSubscriptions.Count; i++)
            {
                _persistentSubscriptions[i]?.Dispose();
            }
            _persistentSubscriptions.Clear();
        }

        protected override void OnDestroy()
        {
            if (View != null)
            {
                try
                {
                    OnDeactivate();
                }
                finally
                {
                    DisposeSubscriptions();
                    DisposePersistentSubscriptions();
                    View = null;
                }
            }
            else
            {
                DisposeSubscriptions();
                DisposePersistentSubscriptions();
            }
            base.OnDestroy();
        }
    }

    /// <summary>
    /// Typed convenience base. The context is the view's
    /// <see cref="CYGViewContext"/> cast to <typeparamref name="TContext"/>.
    /// </summary>
    public abstract class CYGEntityViewComponent<TContext> : CYGEntityViewComponent
        where TContext : CYGViewContext
    {
        public TContext Context => ViewContext as TContext;
    }
}
