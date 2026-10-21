using System.Collections.Generic;
using CookYourGame.Sim;
using UnityEngine;

namespace CookYourGame.SportsArena
{
    /// <summary>Visual adapter for one deterministic arena player.</summary>
    public sealed class SportsArenaPlayerView : MonoBehaviour
    {
        private int _index;
        private int _team;
        private Animator _animator;
        private readonly Dictionary<string, bool> _parameters = new Dictionary<string, bool>();
        private Vector3 _smoothPosition;
        private Quaternion _smoothRotation = Quaternion.identity;
        private float _jumpTime;
        private float _dashTime;
        private float _attackTime;
        private float _catchTime;
        private float _throwTime;
        private float _stunTime;

        public void Initialize(int index, int team)
        {
            _index = index;
            _team = team;
            _smoothPosition = transform.position;
            _smoothRotation = transform.rotation;
            _animator = GetComponentInChildren<Animator>();
            if (_animator != null)
            {
                _animator.applyRootMotion = false;
                AnimatorControllerParameter[] parameters = _animator.parameters;
                for (int i = 0; i < parameters.Length; i++)
                {
                    _parameters[parameters[i].name] = true;
                }
            }
            CreateTeamRing();
        }

        public void Apply(SportsArenaSnapshot snapshot, int index)
        {
            if (index != _index)
            {
                return;
            }

            Vector3 target = SportsArenaConfig.ToVector(snapshot.Positions[index]);
            Quaternion targetRotation = Quaternion.Euler(0f, snapshot.Yaw[index].AsFloat(), 0f);
            float blend = 1f - Mathf.Exp(-28f * Time.deltaTime);
            _smoothPosition = Vector3.Lerp(_smoothPosition, target, blend);
            _smoothRotation = Quaternion.Slerp(_smoothRotation, targetRotation, blend);
            transform.SetPositionAndRotation(_smoothPosition, _smoothRotation);

            byte triggers = snapshot.Triggers[index];
            _jumpTime = Decay(_jumpTime, (triggers & SportsArenaAnim.Jump) != 0, 0.14f);
            _dashTime = Decay(_dashTime, (triggers & SportsArenaAnim.Dash) != 0, 0.18f);
            _attackTime = Decay(_attackTime, (triggers & SportsArenaAnim.Punch) != 0, 0.18f);
            _catchTime = Decay(_catchTime, (triggers & SportsArenaAnim.Catch) != 0, 0.16f);
            _throwTime = Decay(_throwTime, (triggers & (SportsArenaAnim.ThrowShort | SportsArenaAnim.ThrowLong)) != 0, 0.2f);
            _stunTime = Decay(_stunTime, (triggers & SportsArenaAnim.Stun) != 0 || snapshot.Stunned[index], 0.25f);

            SetFloat("Normalized Speed", snapshot.AnimSpeed[index].AsFloat());
            SetBool("Is Grounded", snapshot.Grounded[index]);
            SetBool("Has Ball", snapshot.HasBall[index]);
            SetBool("Block", snapshot.Blocking[index]);
            SetBool("Is Stunned", snapshot.Stunned[index] || _stunTime > 0f);
            SetBool("Is Knockbacked", snapshot.Stunned[index]);
            SetBool("Jump", _jumpTime > 0f);
            SetBool("Dash", _dashTime > 0f);
            SetBool("Attack", _attackTime > 0f);
            SetBool("Catch Ball", _catchTime > 0f);
            SetBool("Throw Ball Short", _throwTime > 0f);
            SetBool("Throw Ball Long", _throwTime > 0f);
        }

        private static float Decay(float value, bool trigger, float duration)
        {
            if (trigger)
            {
                return duration;
            }
            return Mathf.Max(0f, value - Time.deltaTime);
        }

        private void SetFloat(string name, float value)
        {
            if (_animator != null && _parameters.ContainsKey(name))
            {
                _animator.SetFloat(name, value);
            }
        }

        private void SetBool(string name, bool value)
        {
            if (_animator != null && _parameters.ContainsKey(name))
            {
                _animator.SetBool(name, value);
            }
        }

        private void CreateTeamRing()
        {
            GameObject ring = GameObject.CreatePrimitive(PrimitiveType.Cylinder);
            ring.name = "Team Ring";
            Collider collider = ring.GetComponent<Collider>();
            if (collider != null)
            {
                Destroy(collider);
            }
            ring.transform.SetParent(transform, false);
            ring.transform.localPosition = new Vector3(0f, 0.04f, 0f);
            ring.transform.localScale = new Vector3(0.95f, 0.025f, 0.95f);
            Renderer renderer = ring.GetComponent<Renderer>();
            if (renderer != null)
            {
                Shader shader = Shader.Find("Universal Render Pipeline/Unlit");
                if (shader == null)
                {
                    shader = Shader.Find("Unlit/Color");
                }
                Material material = new Material(shader);
                Color color = _team == 0
                    ? new Color(0.15f, 0.45f, 1f)
                    : new Color(1f, 0.2f, 0.15f);
                material.color = color;
                if (material.HasProperty("_BaseColor"))
                {
                    material.SetColor("_BaseColor", color);
                }
                renderer.material = material;
                renderer.shadowCastingMode = UnityEngine.Rendering.ShadowCastingMode.Off;
            }
        }
    }

    /// <summary>Visual adapter for the single deterministic ball.</summary>
    public sealed class SportsArenaBallView : MonoBehaviour
    {
        private Vector3 _smoothPosition;
        private float _spin;

        private void Awake()
        {
            _smoothPosition = transform.position;
        }

        public void Apply(SportsArenaSnapshot snapshot)
        {
            Vector3 target = SportsArenaConfig.ToVector(snapshot.BallPosition);
            float blend = 1f - Mathf.Exp(-35f * Time.deltaTime);
            _smoothPosition = Vector3.Lerp(_smoothPosition, target, blend);
            _spin += Time.deltaTime * 240f;
            transform.SetPositionAndRotation(_smoothPosition, Quaternion.Euler(_spin, _spin * 0.7f, 0f));
        }
    }

    /// <summary>
    /// Original-style 45 degree action camera. It follows the human fighter
    /// directly and leans toward the ball, so moving P1 immediately pans the
    /// view instead of waiting for the average of every fighter to shift.
    /// </summary>
    public static class SportsArenaCamera
    {
        private const float BallBias = 0.28f;

        public static void Follow(Camera camera, SportsArenaSnapshot snapshot)
        {
            Follow(camera, snapshot, 0);
        }

        public static void Follow(Camera camera, SportsArenaSnapshot snapshot, int localPlayerIndex)
        {
            int index = Mathf.Clamp(localPlayerIndex, 0, snapshot.Positions.Length - 1);
            Vector3 local = SportsArenaConfig.ToVector(snapshot.Positions[index]);
            Vector3 ball = SportsArenaConfig.ToVector(snapshot.BallPosition);

            Vector3 flatLocal = new Vector3(local.x, 0f, local.z);
            Vector3 flatBall = new Vector3(ball.x, 0f, ball.z);
            float gap = Vector3.Distance(flatLocal, flatBall);

            Vector3 target = Vector3.Lerp(local, ball, BallBias);
            target.y = 1.35f;

            float distance = Mathf.Clamp(20f + gap * 0.14f, 20f, 26f);
            float height = distance * 0.70710678f;
            Vector3 desired = target + new Vector3(0f, height, -height);

            float blend = 1f - Mathf.Exp(-8f * Time.deltaTime);
            camera.transform.position = Vector3.Lerp(camera.transform.position, desired, blend);
            camera.transform.LookAt(target);
            camera.fieldOfView = 60f;
        }
    }
}
