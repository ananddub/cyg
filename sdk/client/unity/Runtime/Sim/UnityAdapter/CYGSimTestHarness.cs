using System;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Quick test harness for the ECS simulation and view interpolation.
    /// Drop it into an empty scene to exercise local movement, a scripted
    /// remote player, redundant input packets and rollback resimulation
    /// without an external server.
    /// </summary>
    [AddComponentMenu("CookYourGame/Sim/CYG Sim Test Harness")]
    public class CYGSimTestHarness : MonoBehaviour
    {
        [Header("Scene Setup")]
        [Tooltip("Automatically spawns a ground visual plane if missing in scene.")]
        [SerializeField] private bool createGroundPlane = true;

        [Tooltip("Remote bot player id used by this harness.")]
        [SerializeField] private int botPlayerId = 2;

        [Header("Test Controls")]
        [SerializeField] private bool showTestGUI = true;

        private CYGDeterministicRunner _runner;
        private float _remoteBotMoveTimer;
        private bool _botMovingRight = true;

        private void Start()
        {
#if UNITY_2022_2_OR_NEWER
            _runner = FindFirstObjectByType<CYGDeterministicRunner>();
#else
            _runner = FindObjectOfType<CYGDeterministicRunner>();
#endif
            if (_runner == null)
            {
                GameObject runnerGo = new GameObject("[CYG Deterministic Runner]");
                _runner = runnerGo.AddComponent<CYGDeterministicRunner>();
            }

            if (createGroundPlane && GameObject.Find("Sim_GroundPlane") == null)
            {
                GameObject ground = GameObject.CreatePrimitive(PrimitiveType.Plane);
                ground.name = "Sim_GroundPlane";
                ground.transform.position = Vector3.zero;
                ground.transform.localScale = new Vector3(3f, 1f, 3f);

                Renderer renderer = ground.GetComponent<Renderer>();
                if (renderer != null)
                {
                    renderer.material.color = new Color(0.2f, 0.22f, 0.25f);
                }
            }
        }

        private void LateUpdate()
        {
            if (_runner == null || _runner.Session == null)
            {
                return;
            }

            if (!_runner.TryFindEntityByPlayerId(botPlayerId, out _))
            {
                return;
            }

            _remoteBotMoveTimer += Time.deltaTime;
            if (_remoteBotMoveTimer > 1.5f)
            {
                _remoteBotMoveTimer = 0f;
                _botMovingRight = !_botMovingRight;
            }

            short botX = (short)(_botMovingRight ? 600 : -600);
            CYGPlayerInput botInput = new CYGPlayerInput(
                _runner.CurrentSimTick,
                0,
                botX,
                0);
            _runner.InjectPlayerInput(botPlayerId, in botInput);
        }

        private void OnGUI()
        {
            if (!showTestGUI || _runner == null) return;

            GUILayout.BeginArea(new Rect(Screen.width - 250, 10, 240, 220), GUI.skin.box);
            GUILayout.Label("<b>ECS Sim Test Harness</b>");

            if (GUILayout.Button("Spawn Remote Dummy Player"))
            {
                SpawnRemoteDummy();
            }

            if (GUILayout.Button("Late Input (-5 ticks, rollback)"))
            {
                InjectSimulatedRollback();
            }

            if (GUILayout.Button("Teleport Local Player"))
            {
                TeleportLocalPlayer();
            }

            GUILayout.Space(10);
            GUILayout.Label("<size=10>Move: WASD / Arrows\nJump: Space\nAction: LeftShift / Mouse0</size>");
            GUILayout.EndArea();
        }

        private void SpawnRemoteDummy()
        {
            if (_runner == null || _runner.Session == null) return;

            if (_runner.TryFindEntityByPlayerId(botPlayerId, out _))
            {
                return;
            }

            _runner.SpawnPlayer(botPlayerId, new Vector3(3f, 1f, 0f));
        }

        private void InjectSimulatedRollback()
        {
            if (_runner == null || _runner.Session == null) return;

            int pastTick = Math.Max(0, _runner.CurrentSimTick - 5);
            CYGPlayerInput lateInput = new CYGPlayerInput(
                pastTick,
                CYGPlayerInput.ButtonJump,
                0,
                0);
            _runner.InjectPlayerInput(botPlayerId, in lateInput);
        }

        private void TeleportLocalPlayer()
        {
            if (_runner == null || _runner.World == null) return;

            CYGSimAgent agent;
            if (_runner.World.TryGet(_runner.LocalEntity, out agent))
            {
                agent.Position = new FPVector3(FP.Zero, FP.FromInt(3), FP.Zero);
                agent.Velocity = FPVector3.Zero;
                _runner.World.Set(_runner.LocalEntity, agent);
            }
        }
    }
}
