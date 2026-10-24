using System;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Unity visual representation of a deterministic ECS entity.
    /// The simulation supplies fixed-point states at a fixed tick rate; this
    /// component interpolates the Unity transform between the two most recent
    /// ticks. It also owns the view lifecycle: activation/deactivation of
    /// child <see cref="CYGEntityViewComponent"/> scripts, prediction flags,
    /// view-scoped event subscriptions and smooth rollback error correction.
    /// </summary>
    [DisallowMultipleComponent]
    [AddComponentMenu("CookYourGame/Sim/CYG Entity View")]
    public class CYGEntityView : MonoBehaviour
    {
        [Header("Entity Binding")]
        [Tooltip("Deterministic player id this view is bound to (0 for props).")]
        [SerializeField] private int playerId = 0;

        [Header("Interpolation Settings")]
        [Tooltip("Interpolate rotation smoothly via Slerp.")]
        [SerializeField] private bool interpolateRotation = true;

        [Tooltip("Maximum distance jump (meters) before snapping immediately without lerping (e.g. teleports/respawns).")]
        [SerializeField] private float snapDistanceThreshold = 5.0f;

        [Tooltip("Allow gentle extrapolation if Unity frame rate runs slightly ahead of fixed ticks.")]
        [SerializeField] private bool allowExtrapolation = true;

        [Tooltip("Maximum extrapolation time fraction beyond 1.0 (e.g. 0.25 = 25% of a tick).")]
        [SerializeField] private float maxExtrapolationFraction = 0.5f;

        [Header("Error Correction")]
        [Tooltip("Write the simulated transform. Disable when a custom animation or physics script owns the transform.")]
        [SerializeField] private bool updatePosition = true;

        [Tooltip("Spread a rollback correction over a few frames instead of teleporting when the error is small.")]
        [SerializeField] private bool smoothErrorCorrection = true;

        [Tooltip("Largest position error (meters) that is smoothed. Anything bigger snaps immediately.")]
        [SerializeField] private float maxSmoothCorrectionDistance = 3.0f;

        [Tooltip("How quickly a smoothed correction decays back to the authoritative position.")]
        [SerializeField] private float errorCorrectionRate = 10.0f;

        [Header("Debug")]
        [SerializeField] private bool showGizmos = false;

        private Vector3 _previousPosition;
        private Vector3 _currentPosition;
        private Quaternion _previousRotation = Quaternion.identity;
        private Quaternion _currentRotation = Quaternion.identity;
        private Vector3 _currentVelocity;
        private Vector3 _errorOffset;
        private int _health = 100;

        private float _lastTickTime;
        private float _tickInterval = 1f / 60f;
        private bool _isInitialized;
        private bool _hasState;
        private bool _activated;

        private CYGEntityViewComponent[] _components = Array.Empty<CYGEntityViewComponent>();
        private readonly List<IDisposable> _subscriptions = new List<IDisposable>();
        private readonly List<IDisposable> _persistentSubscriptions = new List<IDisposable>();
        private bool _componentsInitialized;
        private bool _initializing;

        #region Properties

        public int PlayerId => playerId;
        public int Health => _health;
        public Vector3 Velocity => _currentVelocity;
        public Vector3 RenderPosition => transform.position;
        public Quaternion RenderRotation => transform.rotation;

        /// <summary>Deterministic entity currently bound to this view.</summary>
        public CYGEntityRef Entity { get; private set; } = CYGEntityRef.None;
        public CYGWorld World { get; private set; }
        public CYGViewContext ViewContext { get; private set; }
        public CYGEntityViewUpdater Updater { get; private set; }

        /// <summary>Deterministic view definition copied from the ECS entity, if any.</summary>
        public CYGViewBinding Definition { get; private set; }
        public bool HasDefinition { get; private set; }

        public bool IsPredicted { get; private set; }
        public bool IsVerified => !IsPredicted;
        public bool IsLocalPlayer { get; private set; }
        public bool IsActive => _activated && isActiveAndEnabled && !Entity.IsNone;
        public bool HasState => _hasState;
        public bool UpdatePosition
        {
            get => updatePosition;
            set => updatePosition = value;
        }

        public CYGEntityViewComponent[] Components => _components;
        public int ComponentCount => _components.Length;

        #endregion

        #region Lifecycle Hooks

        /// <summary>Called once when the component list is first discovered.</summary>
        protected virtual void OnInitialize() { }
        protected virtual void OnActivate() { }
        protected virtual void OnDeactivate() { }
        protected virtual void OnUpdateView() { }
        protected virtual void OnLateUpdateView() { }

        /// <summary>Called when the observed world changes.</summary>
        protected virtual void OnGameChanged() { }

        /// <summary>
        /// Typed event subscription scoped to this view. Subscriptions created
        /// during <see cref="OnInitialize"/> survive pooling and are suppressed
        /// while the view is inactive; subscriptions created during
        /// <see cref="OnActivate"/> are released on deactivate.
        /// </summary>
        protected IDisposable Subscribe<T>(Action<T> handler) where T : CYGEvent
        {
            if (_initializing)
            {
                IDisposable persistent = CYGEvents.Subscribe(this, (T value) =>
                {
                    if (IsActive)
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

        public T GetViewComponent<T>() where T : CYGEntityViewComponent
        {
            for (int i = 0; i < _components.Length; i++)
            {
                if (_components[i] != null && _components[i] is T typed)
                {
                    return typed;
                }
            }
            return null;
        }

        #endregion

        private void Awake()
        {
            InitializeTransform();
        }

        private void Start()
        {
            if (!_isInitialized)
            {
                InitializeTransform();
            }
        }

        private void InitializeTransform()
        {
            _previousPosition = transform.position;
            _currentPosition = transform.position;
            _previousRotation = transform.rotation;
            _currentRotation = transform.rotation;
            _lastTickTime = Time.time;
            _isInitialized = true;
        }

        /// <summary>Binds this visual view to a deterministic player id.</summary>
        public void Bind(int id)
        {
            playerId = id;
        }

        internal void BindEntity(
            CYGEntityRef entity,
            CYGWorld world,
            CYGViewContext context,
            CYGEntityViewUpdater updater)
        {
            Entity = entity;
            World = world;
            ViewContext = context;
            Updater = updater;
            Definition = default(CYGViewBinding);
            HasDefinition = false;
        }

        internal void SetDefinition(in CYGViewBinding definition)
        {
            Definition = definition;
            HasDefinition = true;
        }

        internal void GameChanged()
        {
            OnGameChanged();
            for (int i = 0; i < _components.Length; i++)
            {
                if (_components[i] != null)
                {
                    _components[i].GameChangedInternal();
                }
            }
        }

        internal void SetViewFlags(bool predicted, bool isLocalPlayer)
        {
            IsPredicted = predicted;
            IsLocalPlayer = isLocalPlayer;
        }

        internal void ActivateView()
        {
            if (_activated)
            {
                return;
            }

            EnsureComponents();
            _activated = true;

            for (int i = 0; i < _components.Length; i++)
            {
                if (_components[i] != null)
                {
                    _components[i].BindViewInternal(this);
                }
            }

            OnActivate();
            for (int i = 0; i < _components.Length; i++)
            {
                if (_components[i] != null)
                {
                    _components[i].ActivateInternal();
                }
            }
        }

        internal void DeactivateView()
        {
            if (!_activated)
            {
                return;
            }

            try
            {
                OnDeactivate();
                for (int i = 0; i < _components.Length; i++)
                {
                    if (_components[i] != null)
                    {
                        _components[i].DeactivateInternal();
                    }
                }
            }
            finally
            {
                for (int i = 0; i < _subscriptions.Count; i++)
                {
                    _subscriptions[i]?.Dispose();
                }
                _subscriptions.Clear();

                _activated = false;
                Entity = CYGEntityRef.None;
                World = null;
                Updater = null;
                Definition = default(CYGViewBinding);
                HasDefinition = false;
                IsPredicted = false;
                IsLocalPlayer = false;
            }
        }

        private void EnsureComponents()
        {
            if (_componentsInitialized)
            {
                return;
            }

            _components = GetComponentsInChildren<CYGEntityViewComponent>(true);
            _componentsInitialized = true;
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

        internal void PrepareForReuse()
        {
            _hasState = false;
            _isInitialized = false;
            _errorOffset = Vector3.zero;
            _currentVelocity = Vector3.zero;
            _health = 100;
            Definition = default(CYGViewBinding);
            HasDefinition = false;
        }

        /// <summary>
        /// Applies an authoritative ECS state. Called once per simulation tick.
        /// Set <paramref name="snap"/> after a rollback correction or resync so
        /// the view does not interpolate through the correction; small errors
        /// are still smoothed by the error-correction pass.
        /// </summary>
        public void ApplyState(
            FPVector3 position,
            FPQuaternion rotation,
            FPVector3 velocity,
            int health,
            float tickInterval,
            bool snap = false)
        {
            if (!_isInitialized)
            {
                InitializeTransform();
            }

            _tickInterval = tickInterval > 0f ? tickInterval : (1f / 60f);

            Vector3 newPosition = ToUnity(position);
            Quaternion newRotation = ToUnity(rotation);
            Vector3 renderedPosition = transform.position;

            bool firstState = !_hasState;
            bool hardSnap = snap || firstState;

            if (!hardSnap)
            {
                float sqrDist = (newPosition - _currentPosition).sqrMagnitude;
                hardSnap = sqrDist > snapDistanceThreshold * snapDistanceThreshold;
            }

            _previousPosition = _currentPosition;
            _previousRotation = _currentRotation;
            _currentPosition = newPosition;
            _currentRotation = newRotation;
            _currentVelocity = ToUnity(velocity);
            _health = health;

            if (hardSnap)
            {
                _errorOffset = Vector3.zero;
                if (!firstState && smoothErrorCorrection)
                {
                    float distance = Vector3.Distance(renderedPosition, newPosition);
                    if (distance > 0.0001f && distance <= maxSmoothCorrectionDistance)
                    {
                        _errorOffset = renderedPosition - newPosition;
                    }
                }

                _previousPosition = _currentPosition;
                _previousRotation = _currentRotation;

                if (updatePosition)
                {
                    transform.position = _currentPosition + _errorOffset;
                    transform.rotation = _currentRotation;
                }
            }

            _lastTickTime = Time.time;
            _hasState = true;
        }

        /// <summary>Instantly snaps the view to a position and rotation.</summary>
        public void SnapTo(Vector3 position, Quaternion rotation)
        {
            _previousPosition = position;
            _currentPosition = position;
            _previousRotation = rotation;
            _currentRotation = rotation;
            _errorOffset = Vector3.zero;
            transform.position = position;
            transform.rotation = rotation;
            _lastTickTime = Time.time;
            _hasState = true;
            _isInitialized = true;
        }

        private void Update()
        {
            if (!_activated || Entity.IsNone || World == null)
            {
                return;
            }

            if (_isInitialized && _hasState && _tickInterval > 0f && updatePosition)
            {
                float elapsed = Time.time - _lastTickTime;
                float t = elapsed / _tickInterval;

                if (allowExtrapolation)
                {
                    t = Mathf.Clamp(t, 0f, 1f + maxExtrapolationFraction);
                }
                else
                {
                    t = Mathf.Clamp01(t);
                }

                Vector3 target = Vector3.LerpUnclamped(_previousPosition, _currentPosition, t);

                if (_errorOffset.sqrMagnitude > 0f)
                {
                    float decay = 1f - Mathf.Exp(-Mathf.Max(0.01f, errorCorrectionRate) * Time.deltaTime);
                    _errorOffset = Vector3.Lerp(_errorOffset, Vector3.zero, decay);
                    if (_errorOffset.sqrMagnitude < 0.000001f)
                    {
                        _errorOffset = Vector3.zero;
                    }
                    target += _errorOffset;
                }

                transform.position = target;

                if (interpolateRotation)
                {
                    transform.rotation = Quaternion.Slerp(
                        _previousRotation,
                        _currentRotation,
                        Mathf.Clamp01(t));
                }
            }

            OnUpdateView();
            for (int i = 0; i < _components.Length; i++)
            {
                if (_components[i] != null)
                {
                    _components[i].UpdateViewInternal();
                }
            }
        }

        private void LateUpdate()
        {
            if (!_activated || Entity.IsNone || World == null)
            {
                return;
            }

            OnLateUpdateView();
            for (int i = 0; i < _components.Length; i++)
            {
                if (_components[i] != null)
                {
                    _components[i].LateUpdateViewInternal();
                }
            }
        }

        private void OnDestroy()
        {
            try
            {
                if (_activated)
                {
                    DeactivateView();
                }
            }
            finally
            {
                DisposeSubscriptions();
                DisposePersistentSubscriptions();
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

        private void OnDrawGizmosSelected()
        {
            if (!showGizmos) return;

            Gizmos.color = Color.green;
            Gizmos.DrawWireSphere(_currentPosition, 0.25f);

            Gizmos.color = Color.gray;
            Gizmos.DrawWireSphere(_previousPosition, 0.2f);

            if (_currentVelocity.sqrMagnitude > 0.01f)
            {
                Gizmos.color = Color.cyan;
                Gizmos.DrawRay(transform.position, _currentVelocity);
            }
        }

        private static Vector3 ToUnity(FPVector3 value)
        {
            return new Vector3(value.x.AsFloat(), value.y.AsFloat(), value.z.AsFloat());
        }

        private static Quaternion ToUnity(FPQuaternion value)
        {
            float x = value.x.AsFloat();
            float y = value.y.AsFloat();
            float z = value.z.AsFloat();
            float w = value.w.AsFloat();
            if (x == 0f && y == 0f && z == 0f && w == 0f)
            {
                return Quaternion.identity;
            }
            return new Quaternion(x, y, z, w);
        }
    }
}
