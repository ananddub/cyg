using System;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Quick test harness component for testing deterministic quantum simulation and view interpolation.
    /// Can be dropped into any empty Unity scene to test local movement, remote input replication,
    /// and rollback prediction without running an external server.
    /// </summary>
    [AddComponentMenu("CookYourGame/Sim/CYG Sim Test Harness")]
    public class CYGSimTestHarness : MonoBehaviour
    {
        [Header("Scene Setup")]
        [Tooltip("Automatically spawns a ground visual plane if missing in scene.")]
        [SerializeField] private bool createGroundPlane = true;

        [Tooltip("Prefab for spawned player entities (leave null to use default cubes).")]
        [SerializeField] private GameObject playerPrefab;

        [Header("Test Controls")]
        [SerializeField] private bool showTestGUI = true;

        private CYGDeterministicRunner _runner;
        private int _dummyRemotePlayerId = 2;
        private float _remoteBotMoveTimer = 0f;
        private bool _botMovingRight = true;

        private void Start()
        {
            // 1. Locate or create CYGDeterministicRunner. FindFirstObjectByType
            // is Unity 2022.2+ but the package declares 2021.3 support.
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

            // 2. Optionally create visual ground plane at Y = 0
            if (createGroundPlane && GameObject.Find("Sim_GroundPlane") == null)
            {
                GameObject ground = GameObject.CreatePrimitive(PrimitiveType.Plane);
                ground.name = "Sim_GroundPlane";
                ground.transform.position = Vector3.zero;
                ground.transform.localScale = new Vector3(3f, 1f, 3f);

                // Give it a dark grid-like material if available
                Renderer rend = ground.GetComponent<Renderer>();
                if (rend != null)
                {
                    rend.material.color = new Color(0.2f, 0.22f, 0.25f);
                }
            }
        }

        private void Update()
        {
            // Simulate bot input if remote dummy player is active
            if (_runner != null && _runner.RollbackRunner != null)
            {
                if (_runner.RollbackRunner.TryGetEntityState(_dummyRemotePlayerId, out _))
                {
                    _remoteBotMoveTimer += Time.deltaTime;
                    if (_remoteBotMoveTimer > 1.5f)
                    {
                        _remoteBotMoveTimer = 0f;
                        _botMovingRight = !_botMovingRight;
                    }

                    short botX = (short)(_botMovingRight ? 600 : -600);
                    CYGPlayerInput botInput = new CYGPlayerInput(_runner.CurrentSimTick, 0, botX, 0);
                    _runner.RollbackRunner.OnRemoteInputReceived(_dummyRemotePlayerId, botInput);
                }
            }
        }

        private void OnGUI()
        {
            if (!showTestGUI || _runner == null) return;

            GUILayout.BeginArea(new Rect(Screen.width - 240, 10, 230, 200), GUI.skin.box);
            GUILayout.Label("<b>reference Sim Test Harness</b>");

            if (GUILayout.Button("Spawn Remote Dummy Player"))
            {
                SpawnRemoteDummy();
            }

            if (GUILayout.Button("Simulate Rollback (-5 ticks)"))
            {
                InjectSimulatedRollback();
            }

            if (GUILayout.Button("Reset Simulation"))
            {
                _runner.RollbackRunner.Reset(0);
            }

            GUILayout.Space(10);
            GUILayout.Label("<size=10>Move: WASD / Arrows\nJump: Space\nAction: LeftShift / Mouse0</size>");
            GUILayout.EndArea();
        }

        private void SpawnRemoteDummy()
        {
            if (_runner == null || _runner.RollbackRunner == null) return;

            if (!_runner.RollbackRunner.TryGetEntityState(_dummyRemotePlayerId, out _))
            {
                FPVector3 spawnPos = new FPVector3(FP.FromInt(3), FP.FromInt(1), FP.Zero);
                _runner.RollbackRunner.SetEntityState(new CYGEntityState(_dummyRemotePlayerId, spawnPos));
            }
        }

        private void InjectSimulatedRollback()
        {
            if (_runner == null || _runner.RollbackRunner == null) return;

            int pastTick = Math.Max(0, _runner.CurrentSimTick - 5);
            // Send dummy player jump input at a past tick to trigger resimulation
            CYGPlayerInput lateInput = new CYGPlayerInput(pastTick, CYGPlayerInput.ButtonJump, 0, 0);
            _runner.RollbackRunner.OnRemoteInputReceived(_dummyRemotePlayerId, lateInput);
        }
    }
}
