using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Presentation-only driver that maps deterministic simulation state to a
    /// Unity <see cref="Animator"/>. It reads <see cref="CYGSimAgent"/> every
    /// visual frame and writes speed, grounded, vertical speed and an attack
    /// trigger, so walk/run/jump/fall transitions follow the ECS world instead
    /// of Unity input or Rigidbody state.
    ///
    /// Use this for local simulation presentation. Do not stack it with
    /// <see cref="CYGAnimatorSync"/> on the same Animator unless you want the
    /// network sync to override these values on remote peers.
    /// </summary>
    [AddComponentMenu("CookYourGame/Sim/Sim Animation Driver")]
    public sealed class CYGSimAnimationDriver : CYGEntityViewComponent
    {
        [Header("Animator")]
        [Tooltip("Animator to drive. Falls back to an Animator on this object or its children.")]
        [SerializeField] private Animator animator;

        [Tooltip("Smooth presentation speed changes. Simulation values stay fixed-point.")]
        [SerializeField] private bool smoothSpeed = true;

        [SerializeField] private float speedSmoothing = 12f;

        [Tooltip("Reset driven parameters when the view is pooled or destroyed.")]
        [SerializeField] private bool resetOnDeactivate = true;

        [Header("Parameters")]
        [SerializeField] private string speedParameter = "Speed";
        [SerializeField] private string groundedParameter = "Grounded";
        [SerializeField] private string verticalSpeedParameter = "VerticalSpeed";
        [SerializeField] private string attackTrigger = "Attack";

        [Header("Mapping")]
        [Tooltip("Multiplier applied to the simulation's horizontal speed before writing the Speed parameter.")]
        [SerializeField] private float speedScale = 1f;

        [Tooltip("Write the grounded flag. Disable when the Animator derives it from Speed.")]
        [SerializeField] private bool writeGrounded = true;

        [Tooltip("Write vertical velocity so jump/fall blends can react to it.")]
        [SerializeField] private bool writeVerticalSpeed = true;

        private readonly HashSet<int> _parameters = new HashSet<int>();
        private int _speedHash;
        private int _groundedHash;
        private int _verticalHash;
        private int _attackHash;
        private float _displayedSpeed;
        private bool _wasAttacking;
        private bool _initialized;

        public Animator Animator => animator;

        protected override void OnInitialize()
        {
            if (animator == null)
            {
                animator = GetComponentInChildren<Animator>(true);
            }
            if (animator == null)
            {
                Debug.LogWarning(
                    "[CYG] Sim animation driver on '" + name + "' has no Animator; disabling.", this);
                enabled = false;
                return;
            }

            CacheParameters();
            _initialized = true;
        }

        protected override void OnActivate(CYGWorld world, CYGEntityRef entity)
        {
            _displayedSpeed = 0f;
            CYGSimAgent agent;
            _wasAttacking = world != null && world.TryGet(entity, out agent) && agent.Attacking;
            if (resetOnDeactivate)
            {
                WriteSpeed(0f);
            }
        }

        protected override void OnUpdateView()
        {
            if (!_initialized || animator == null || World == null)
            {
                return;
            }

            CYGSimAgent agent;
            if (!World.TryGet(Entity, out agent))
            {
                return;
            }

            float target = HorizontalSpeed(agent) * speedScale;
            if (smoothSpeed)
            {
                _displayedSpeed = Mathf.MoveTowards(
                    _displayedSpeed,
                    target,
                    Mathf.Max(0f, speedSmoothing) * Time.deltaTime);
            }
            else
            {
                _displayedSpeed = target;
            }
            WriteSpeed(_displayedSpeed);

            if (writeGrounded && _parameters.Contains(_groundedHash))
            {
                animator.SetBool(_groundedHash, agent.Grounded);
            }

            if (writeVerticalSpeed && _parameters.Contains(_verticalHash))
            {
                animator.SetFloat(_verticalHash, agent.Velocity.y.AsFloat());
            }

            // Attack is simulation state: fire the trigger exactly once on the
            // rising edge so rollback/replay cannot repeat or drop an attack.
            if (agent.Attacking && !_wasAttacking)
            {
                TriggerAttack();
            }
            _wasAttacking = agent.Attacking;
        }

        /// <summary>Fires the configured attack trigger on the next visual update.</summary>
        public void TriggerAttack()
        {
            if (animator != null && _parameters.Contains(_attackHash))
            {
                animator.SetTrigger(_attackHash);
            }
        }

        protected override void OnDeactivate()
        {
            _displayedSpeed = 0f;
            _wasAttacking = false;
            if (!resetOnDeactivate || animator == null)
            {
                return;
            }

            WriteSpeed(0f);
            if (_parameters.Contains(_groundedHash))
            {
                animator.SetBool(_groundedHash, true);
            }
            if (_parameters.Contains(_verticalHash))
            {
                animator.SetFloat(_verticalHash, 0f);
            }
        }

        private void CacheParameters()
        {
            _speedHash = Animator.StringToHash(speedParameter);
            _groundedHash = Animator.StringToHash(groundedParameter);
            _verticalHash = Animator.StringToHash(verticalSpeedParameter);
            _attackHash = Animator.StringToHash(attackTrigger);

            _parameters.Clear();
            AnimatorControllerParameter[] parameters = animator.parameters;
            for (int i = 0; i < parameters.Length; i++)
            {
                _parameters.Add(parameters[i].nameHash);
            }

            WarnMissing(_speedHash, speedParameter);
            WarnMissing(_groundedHash, groundedParameter);
            WarnMissing(_verticalHash, verticalSpeedParameter);
            WarnMissing(_attackHash, attackTrigger);
        }

        private void WarnMissing(int hash, string parameter)
        {
            if (!string.IsNullOrEmpty(parameter) && !_parameters.Contains(hash))
            {
                Debug.LogWarning(
                    "[CYG] Animator on '" + name + "' has no parameter '" + parameter + "'.", this);
            }
        }

        private void WriteSpeed(float value)
        {
            if (animator != null && _parameters.Contains(_speedHash))
            {
                animator.SetFloat(_speedHash, value);
            }
        }

        private static float HorizontalSpeed(in CYGSimAgent agent)
        {
            FP squared = agent.Velocity.x * agent.Velocity.x
                + agent.Velocity.z * agent.Velocity.z;
            return squared > FP.Zero ? FP.Sqrt(squared).AsFloat() : 0f;
        }
    }
}
