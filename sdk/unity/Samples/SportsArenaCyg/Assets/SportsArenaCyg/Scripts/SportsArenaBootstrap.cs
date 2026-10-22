using System;
using System.Collections.Generic;
using CookYourGame.Sim;
using UnityEngine;
using UnityEngine.InputSystem;
using UnityEngine.InputSystem.Controls;

namespace CookYourGame.SportsArena
{
    /// <summary>
    /// Runtime bootstrap for the CYG-native Sports Arena Brawler port. It keeps
    /// the original art/prefabs, removes the reference dependency, builds static
    /// collision from the environment and hosts the deterministic simulation.
    /// </summary>
    [DisallowMultipleComponent]
    public sealed partial class SportsArenaBootstrap : MonoBehaviour
    {
        public static SportsArenaBootstrap Instance { get; private set; }

        [Header("Art")]
        public GameObject environmentPrefab;
        public GameObject playerPrefab;
        public GameObject ballPrefab;

        [Header("Rules")]
        public bool playerTwoIsBot = true;
        public bool showHud = true;

        public SportsArenaSimulation Simulation { get; private set; }
        public SportsArenaPlayerView[] PlayerViews { get; private set; }
        public SportsArenaBallView BallView { get; private set; }
        public Camera ArenaCamera { get; private set; }
        public int Tick { get; private set; }

        private readonly SportsArenaInput[] _inputs = new SportsArenaInput[SportsArenaConfig.PlayerCount];
        private GameObject _environment;
        private bool _ready;

        private void Awake()
        {
            if (Instance != null && Instance != this)
            {
                Destroy(gameObject);
                return;
            }
            Instance = this;
            Time.fixedDeltaTime = 1f / SportsArenaConfig.TickRate;
            Application.targetFrameRate = 120;
            InitializeNetworkMode();
            Build();
        }

        private void OnDestroy()
        {
            ShutdownNetworkMode();
            if (Instance == this)
            {
                Instance = null;
            }
        }

        private void FixedUpdate()
        {
            if (!_ready)
            {
                return;
            }

            if (networkMode)
            {
                // Network mode owns both the tick counter and the step gate.
                // It only advances the simulation when both human inputs for
                // the exact tick are present.
                StepNetworkFrame();
                return;
            }

            Tick++;
            for (int i = 0; i < _inputs.Length; i++)
            {
                _inputs[i] = ReadInput(i, Tick);
            }
            Simulation.TickStep(_inputs);
        }

        private void LateUpdate()
        {
            if (!_ready)
            {
                return;
            }

            SportsArenaSnapshot snapshot = Simulation.Snapshot;
            for (int i = 0; i < PlayerViews.Length; i++)
            {
                PlayerViews[i].Apply(snapshot, i);
            }
            BallView.Apply(snapshot);
            if (ArenaCamera != null)
            {
                SportsArenaCamera.Follow(ArenaCamera, snapshot, LocalPlayerIndex);
            }
        }

        private void Build()
        {
            environmentPrefab = environmentPrefab != null
                ? environmentPrefab
                : Resources.Load<GameObject>("Environment");
            playerPrefab = playerPrefab != null
                ? playerPrefab
                : Resources.Load<GameObject>("Player");
            ballPrefab = ballPrefab != null
                ? ballPrefab
                : Resources.Load<GameObject>("Ball");

            _environment = Instantiate(environmentPrefab != null ? environmentPrefab : new GameObject("Empty Arena"));
            _environment.name = "Arena Environment";
            StripCameras(_environment);

            Transform[] allTransforms = _environment.GetComponentsInChildren<Transform>(true);
            Bounds bounds = ComputeBounds(_environment);
            FPAABB arena = new FPAABB(
                SportsArenaConfig.ToFixed(new Vector3(bounds.min.x, -1f, bounds.min.z)),
                SportsArenaConfig.ToFixed(new Vector3(bounds.max.x, 8f, bounds.max.z)));

            Simulation = new SportsArenaSimulation();
            AddEnvironmentCollision(_environment, bounds);
            AddArenaBounds(arena);

            SportsArenaGoal[] goals = FindGoals(_environment, bounds, arena);
            FPVector3[] spawns = FindSpawners(allTransforms, goals, arena);
            FPVector3 ballSpawn = FindBallSpawn(allTransforms, arena);

            if (networkMode)
            {
                playerTwoIsBot = false;
            }

            Simulation.Start(arena, goals, spawns, ballSpawn);
            Debug.Log($"[SportsArena] Arena {arena.Min}..{arena.Max}, goals={goals.Length}, ball={ballSpawn}, spawn0={spawns[0]}, spawn1={spawns[1]}");
            if (playerTwoIsBot)
            {
                Simulation.Players[1].IsBot = true;
            }
            for (int i = 2; i < Simulation.Players.Length; i++)
            {
                Simulation.Players[i].IsBot = true;
            }

            PlayerViews = new SportsArenaPlayerView[SportsArenaConfig.PlayerCount];
            for (int i = 0; i < PlayerViews.Length; i++)
            {
                GameObject instance = Instantiate(playerPrefab);
                int team = Simulation.Players[i].Team;
                instance.name = (team == 0 ? "Player Blue " : "Player Red ") + (i / 2 + 1);
                StripCameras(instance);
                PlayerViews[i] = instance.GetComponent<SportsArenaPlayerView>();
                if (PlayerViews[i] == null)
                {
                    PlayerViews[i] = instance.AddComponent<SportsArenaPlayerView>();
                }
                PlayerViews[i].Initialize(i, Simulation.Players[i].Team);
            }

            GameObject ballInstance = Instantiate(ballPrefab);
            ballInstance.name = "Arena Ball";
            StripCameras(ballInstance);
            BallView = ballInstance.GetComponent<SportsArenaBallView>();
            if (BallView == null)
            {
                BallView = ballInstance.AddComponent<SportsArenaBallView>();
            }

            ArenaCamera = FindOrCreateCamera();
            if (showHud)
            {
                gameObject.AddComponent<SportsArenaHud>();
            }

            _ready = true;
            if (networkMode)
            {
                StartNetworkMode();
            }
        }

    }
}
