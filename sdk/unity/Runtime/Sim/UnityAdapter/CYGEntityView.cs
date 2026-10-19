using System;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Unity visual representation of a deterministic simulation entity.
    /// In reference architecture, Unity is purely the renderer/view: the simulation
    /// runs at a fixed tick rate (e.g. 60Hz), while Unity renders at variable high
    /// frame rates (60/120/144Hz+).
    ///
    /// This component smoothly interpolates transform.position and transform.rotation
    /// between PreviousState and CurrentState based on the elapsed tick fraction.
    /// </summary>
    [DisallowMultipleComponent]
    [AddComponentMenu("CookYourGame/Sim/CYG Entity View")]
    public class CYGEntityView : MonoBehaviour
    {
        [Header("Entity Binding")]
        [Tooltip("Deterministic EntityId this visual GameObject binds to.")]
        [SerializeField] private int entityId = 0;

        [Header("Interpolation Settings")]
        [Tooltip("Interpolate rotation smoothly via Slerp.")]
        [SerializeField] private bool interpolateRotation = true;

        [Tooltip("Maximum distance jump (meters) before snapping immediately without lerping (e.g. teleports/respawns).")]
        [SerializeField] private float snapDistanceThreshold = 5.0f;

        [Tooltip("Allow gentle extrapolation if Unity frame rate runs slightly ahead of fixed ticks.")]
        [SerializeField] private bool allowExtrapolation = true;

        [Tooltip("Maximum extrapolation time fraction beyond 1.0 (e.g. 0.25 = 25% of a tick).")]
        [SerializeField] private float maxExtrapolationFraction = 0.5f;

        [Header("Debug")]
        [SerializeField] private bool showGizmos = false;

        // Visual render states (Unity floats)
        private Vector3 _previousPosition;
        private Vector3 _currentPosition;
        private Quaternion _previousRotation = Quaternion.identity;
        private Quaternion _currentRotation = Quaternion.identity;
        private Vector3 _currentVelocity;
        private int _health = 100;

        private float _lastTickTime;
        private float _tickInterval = 1f / 60f;
        private bool _isInitialized;

        #region Properties

        /// <summary>The simulation entity ID bound to this view.</summary>
        public int EntityId => entityId;

        /// <summary>Latest simulation health reported for this entity.</summary>
        public int Health => _health;

        /// <summary>Latest simulation velocity in meters per second.</summary>
        public Vector3 Velocity => _currentVelocity;

        /// <summary>Interpolated render position.</summary>
        public Vector3 RenderPosition => transform.position;

        /// <summary>Interpolated render rotation.</summary>
        public Quaternion RenderRotation => transform.rotation;

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

        /// <summary>
        /// Binds this visual view to a deterministic simulation EntityId.
        /// </summary>
        public void Bind(int id)
        {
            entityId = id;
        }

        /// <summary>
        /// Called by CYGDeterministicRunner at the end of each simulation tick.
        /// Updates the target state from pure fixed-point CYGEntityState.
        /// </summary>
        public void OnSimulationTick(in CYGEntityState state, float tickInterval)
        {
            if (!_isInitialized)
            {
                InitializeTransform();
            }

            _tickInterval = tickInterval > 0f ? tickInterval : (1f / 60f);

            // Shift current to previous
            _previousPosition = _currentPosition;
            _previousRotation = _currentRotation;

            // Convert Q32.32 fixed-point vectors and quaternions to Unity floats
            _currentPosition = new Vector3(
                state.Position.x.AsFloat(),
                state.Position.y.AsFloat(),
                state.Position.z.AsFloat()
            );

            _currentRotation = new Quaternion(
                state.Rotation.x.AsFloat(),
                state.Rotation.y.AsFloat(),
                state.Rotation.z.AsFloat(),
                state.Rotation.w.AsFloat()
            );

            // Normalize quaternion in case of precision drift
            if (_currentRotation.x == 0f && _currentRotation.y == 0f && _currentRotation.z == 0f && _currentRotation.w == 0f)
            {
                _currentRotation = Quaternion.identity;
            }

            _currentVelocity = new Vector3(
                state.Velocity.x.AsFloat(),
                state.Velocity.y.AsFloat(),
                state.Velocity.z.AsFloat()
            );

            _health = state.Health;

            // Teleport / Respawn snap detection:
            // If the position jumped more than threshold, don't lerp across the entire world
            float sqrDist = (_currentPosition - _previousPosition).sqrMagnitude;
            if (sqrDist > snapDistanceThreshold * snapDistanceThreshold)
            {
                _previousPosition = _currentPosition;
                _previousRotation = _currentRotation;
                transform.position = _currentPosition;
                transform.rotation = _currentRotation;
            }

            _lastTickTime = Time.time;
        }

        /// <summary>
        /// Updates the target states with explicit previous and current simulation snapshots.
        /// Ideal when reading from circular state history buffers (e.g. TryGetEntityInterpolationStates).
        /// </summary>
        public void OnSimulationTick(in CYGEntityState previousState, in CYGEntityState currentState, float tickInterval)
        {
            if (!_isInitialized)
            {
                InitializeTransform();
            }

            _tickInterval = tickInterval > 0f ? tickInterval : (1f / 60f);

            _previousPosition = new Vector3(
                previousState.Position.x.AsFloat(),
                previousState.Position.y.AsFloat(),
                previousState.Position.z.AsFloat()
            );

            _previousRotation = new Quaternion(
                previousState.Rotation.x.AsFloat(),
                previousState.Rotation.y.AsFloat(),
                previousState.Rotation.z.AsFloat(),
                previousState.Rotation.w.AsFloat()
            );

            _currentPosition = new Vector3(
                currentState.Position.x.AsFloat(),
                currentState.Position.y.AsFloat(),
                currentState.Position.z.AsFloat()
            );

            _currentRotation = new Quaternion(
                currentState.Rotation.x.AsFloat(),
                currentState.Rotation.y.AsFloat(),
                currentState.Rotation.z.AsFloat(),
                currentState.Rotation.w.AsFloat()
            );

            if (_currentRotation.x == 0f && _currentRotation.y == 0f && _currentRotation.z == 0f && _currentRotation.w == 0f)
            {
                _currentRotation = Quaternion.identity;
            }
            if (_previousRotation.x == 0f && _previousRotation.y == 0f && _previousRotation.z == 0f && _previousRotation.w == 0f)
            {
                _previousRotation = Quaternion.identity;
            }

            _currentVelocity = new Vector3(
                currentState.Velocity.x.AsFloat(),
                currentState.Velocity.y.AsFloat(),
                currentState.Velocity.z.AsFloat()
            );

            _health = currentState.Health;

            float sqrDist = (_currentPosition - _previousPosition).sqrMagnitude;
            if (sqrDist > snapDistanceThreshold * snapDistanceThreshold)
            {
                _previousPosition = _currentPosition;
                _previousRotation = _currentRotation;
                transform.position = _currentPosition;
                transform.rotation = _currentRotation;
            }

            _lastTickTime = Time.time;
        }

        /// <summary>
        /// Instantly snaps the view to a given position and rotation without interpolation.
        /// </summary>
        public void SnapTo(Vector3 position, Quaternion rotation)
        {
            _previousPosition = position;
            _currentPosition = position;
            _previousRotation = rotation;
            _currentRotation = rotation;
            transform.position = position;
            transform.rotation = rotation;
            _lastTickTime = Time.time;
        }

        private void Update()
        {
            if (!_isInitialized || _tickInterval <= 0f) return;

            // Calculate interpolation fraction t between [PreviousState, CurrentState]
            // t = (Time.time - lastTickTime) / tickInterval
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

            // Smooth position interpolation
            transform.position = Vector3.LerpUnclamped(_previousPosition, _currentPosition, t);

            // Smooth rotation interpolation
            if (interpolateRotation)
            {
                transform.rotation = Quaternion.Slerp(_previousRotation, _currentRotation, Mathf.Clamp01(t));
            }
        }

        private void OnDrawGizmosSelected()
        {
            if (!showGizmos) return;

            // Current target simulation position
            Gizmos.color = Color.green;
            Gizmos.DrawWireSphere(_currentPosition, 0.25f);

            // Previous simulation position
            Gizmos.color = Color.gray;
            Gizmos.DrawWireSphere(_previousPosition, 0.2f);

            // Velocity ray
            if (_currentVelocity.sqrMagnitude > 0.01f)
            {
                Gizmos.color = Color.cyan;
                Gizmos.DrawRay(transform.position, _currentVelocity);
            }
        }
    }
}
