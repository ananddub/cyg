using UnityEngine;
using UnityEngine.InputSystem;
using CookYourGame.Sim;
using CookYourGame.Sim.TwoBox;

namespace CookYourGame
{
    /// <summary>
    /// Pure reference-Style View Controller with AutoBoot, Bot AI, Match Countdown & Desync Diagnostics.
    /// Inherits from CYGCallbacks (mirrors referenceCallbacks / referenceMonoBehaviour).
    /// All simulation logic lives 100% in TwoBoxMovementSystem & GemCollisionSystem.
    /// Auto-boots on Play mode in any scene; supports Human vs Human or Human vs Bot.
    /// Integrated with CYGCountdownTimer, CYGDesyncOverlay (F8), and CYGStaticMeshAuthoring.
    /// </summary>
    public class CYGTwoBoxGame : CYGCallbacks
    {
        [Header("Scores (Event Driven)")]
        public int scoreP1;
        public int scoreP2;

        [Header("Bot AI")]
        public bool p2IsBot = true;

        [Header("Match Timer (CYGCountdownTimer)")]
        public float roundDurationSeconds = 60f;
        private CYGCountdownTimer _roundTimer;

        private GameObject _gemVisual;
        private CYGEntityView _p2View;
        private CYGEntityView _p1View;

        protected override void Awake()
        {
            base.Awake();
            _roundTimer = CYGCountdownTimer.Create(FP.FromFloat(roundDurationSeconds), loop: false, start: true);
            SetupArena();

            if (Runner != null)
            {
                Runner.SimulationMode = CYGSimulationMode.Custom;
                Runner.SyncAuthority = CYGSyncAuthority.LocalAuthority;
                if (!Runner.IsSimulationStarted)
                {
                    Runner.StartSimulation();
                }
            }
        }

        protected override void OnInitSystems(CYGSystemRunner systems)
        {
            systems.AddSystem(new TwoBoxMovementSystem(), order: 10);
            systems.AddSystem(new GemCollisionSystem(), order: 20);
        }

        protected override void OnGameStart()
        {
            SetupArena();
            Subscribe<GemCollectedEvent>(OnGemCollected);
        }

        protected override void OnInputPolling(CYGPlayerRef player)
        {
            // Advance deterministic round timer
            FP dt = Runner != null && Runner.Session != null ? Runner.Session.DeltaTime : FP.FromFraction(1, 60);
            _roundTimer.Tick(dt);

            // 1. Read Player 1 Input (WASD)
            short x1 = 0, y1 = 0;
            var kb = Keyboard.current;
            if (kb != null)
            {
                if (kb.aKey.isPressed) x1 -= 1000;
                if (kb.dKey.isPressed) x1 += 1000;
                if (kb.sKey.isPressed) y1 -= 1000;
                if (kb.wKey.isPressed) y1 += 1000;
            }
            else
            {
                var wasd = ReadMoveVectorWASD();
                x1 = wasd.x;
                y1 = wasd.y;
            }

            if (x1 != 0 && y1 != 0)
            {
                x1 = (short)(x1 * 0.7071f);
                y1 = (short)(y1 * 0.7071f);
            }
            SubmitInput(new CYGPlayerInput(InputTick, 0, x1, y1));

            // 2. Read Player 2 Input (Arrow Keys, or Bot AI if idle)
            short x2 = 0, y2 = 0;
            if (kb != null)
            {
                if (kb.leftArrowKey.isPressed) x2 -= 1000;
                if (kb.rightArrowKey.isPressed) x2 += 1000;
                if (kb.downArrowKey.isPressed) y2 -= 1000;
                if (kb.upArrowKey.isPressed) y2 += 1000;
            }
            else
            {
                var arrows = ReadMoveVectorArrows();
                x2 = arrows.x;
                y2 = arrows.y;
            }

            if (x2 != 0 && y2 != 0)
            {
                x2 = (short)(x2 * 0.7071f);
                y2 = (short)(y2 * 0.7071f);
            }

            // If player 2 is not touching arrow keys and bot is enabled, Bot AI chases the gem
            if (x2 == 0 && y2 == 0 && p2IsBot && _gemVisual != null)
            {
                Vector3 p2Pos = _p2View != null ? _p2View.transform.position : new Vector3(3.5f, 0.5f, 0f);
                Vector3 diff = _gemVisual.transform.position - p2Pos;
                if (diff.x > 0.3f) x2 = 1000;
                else if (diff.x < -0.3f) x2 = -1000;
                if (diff.z > 0.3f) y2 = 1000;
                else if (diff.z < -0.3f) y2 = -1000;
            }

            SetPlayerInput(2, new CYGPlayerInput(InputTick, 0, x2, y2));
        }

        protected override void OnViewSpawned(CYGEntityView view)
        {
            if (view.PlayerId == 1)
            {
                _p1View = view;
            }
            else if (view.PlayerId == 2)
            {
                _p2View = view;
            }

            // Box appearance: 1.2m cube, Vibrant Blue (P1) vs Vibrant Orange (P2)
            view.transform.localScale = Vector3.one * 1.2f;
            var rend = view.GetComponent<Renderer>();
            if (rend != null)
            {
                rend.material.color = view.PlayerId == 1
                    ? new Color(0.15f, 0.65f, 1f)   // Blue P1
                    : new Color(1f, 0.45f, 0.1f);   // Orange P2
            }
        }

        protected override void OnViewUpdate()
        {
            if (_gemVisual != null)
            {
                _gemVisual.transform.Rotate(new Vector3(30f, 60f, 45f) * Time.deltaTime);
            }
        }

        private void OnGemCollected(GemCollectedEvent e)
        {
            if (e.Player.Value == 1) scoreP1 = e.NewScore;
            if (e.Player.Value == 2) scoreP2 = e.NewScore;

            if (_gemVisual != null)
            {
                _gemVisual.transform.position = new Vector3(e.Position.x.AsFloat(), e.Position.y.AsFloat(), e.Position.z.AsFloat());
            }
        }

        private void SetupArena()
        {
            Camera cam = Camera.main;
            if (cam != null)
            {
                // Disable CinemachineBrain if attached so it doesn't hijack camera in LateUpdate
                var cmBrain = cam.GetComponent("CinemachineBrain");
                if (cmBrain is Behaviour b)
                {
                    b.enabled = false;
                }

                // Top-Down Orthographic Arcade Camera (Zero perspective distortion, clean flat arena)
                cam.transform.SetPositionAndRotation(new Vector3(0f, 16f, 0f), Quaternion.Euler(90f, 0f, 0f));
                cam.orthographic = true;
                cam.orthographicSize = 7.5f;
                cam.nearClipPlane = 0.1f;
                cam.farClipPlane = 50f;
                cam.clearFlags = CameraClearFlags.SolidColor;
                cam.backgroundColor = new Color(0.08f, 0.09f, 0.12f);
            }

            // Disable any Cinemachine cameras in the scene
            GameObject cmCam = GameObject.Find("CinemachineCamera");
            if (cmCam != null)
            {
                cmCam.SetActive(false);
            }

            if (GameObject.Find("TwoBox_Arena_Floor") == null)
            {
                GameObject floor = GameObject.CreatePrimitive(PrimitiveType.Plane);
                floor.name = "TwoBox_Arena_Floor";
                floor.transform.position = Vector3.zero;
                floor.transform.localScale = new Vector3(1.4f, 1f, 1.4f);
                var rend = floor.GetComponent<Renderer>();
                if (rend != null) rend.material.color = new Color(0.12f, 0.14f, 0.18f);
            }

            if (_gemVisual == null)
            {
                _gemVisual = GameObject.CreatePrimitive(PrimitiveType.Cube);
                _gemVisual.name = "Golden_Gem_Target";
                _gemVisual.transform.position = new Vector3(0f, 0.6f, 0f);
                _gemVisual.transform.localScale = Vector3.one * 0.7f;
                Destroy(_gemVisual.GetComponent<Collider>());
                var rend = _gemVisual.GetComponent<Renderer>();
                if (rend != null) rend.material.color = new Color(1f, 0.85f, 0.1f);
            }

            // Add Arena Static Obstacles with CYGStaticMeshAuthoring
            CreateArenaObstacle("Arena_Obstacle_North", new Vector3(0f, 0.5f, 5.8f), new Vector3(6f, 1f, 0.8f));
            CreateArenaObstacle("Arena_Obstacle_South", new Vector3(0f, 0.5f, -5.8f), new Vector3(6f, 1f, 0.8f));
        }

        private void CreateArenaObstacle(string name, Vector3 pos, Vector3 size)
        {
            if (GameObject.Find(name) != null) return;
            GameObject obstacle = GameObject.CreatePrimitive(PrimitiveType.Cube);
            obstacle.name = name;
            obstacle.transform.position = pos;
            obstacle.transform.localScale = size;
            var rend = obstacle.GetComponent<Renderer>();
            if (rend != null) rend.material.color = new Color(0.2f, 0.25f, 0.32f);

            // Attach static mesh authoring for deterministic physics world
            if (obstacle.GetComponent<CYGStaticMeshAuthoring>() == null)
            {
                obstacle.AddComponent<CYGStaticMeshAuthoring>();
            }
        }

        private void LateUpdate()
        {
            // Keep CinemachineBrain disabled and camera firmly locked top-down
            Camera cam = Camera.main;
            if (cam != null)
            {
                var cmBrain = cam.GetComponent("CinemachineBrain");
                if (cmBrain is Behaviour b && b.enabled)
                {
                    b.enabled = false;
                }

                if (!cam.orthographic || cam.transform.position.y < 10f)
                {
                    cam.transform.SetPositionAndRotation(new Vector3(0f, 16f, 0f), Quaternion.Euler(90f, 0f, 0f));
                    cam.orthographic = true;
                    cam.orthographicSize = 7.5f;
                }
            }
        }

        private void OnGUI()
        {
            // Top HUD Banner
            Rect banner = new Rect((Screen.width - 550) / 2f, 15f, 550f, 75f);
            GUI.Box(banner, GUIContent.none);
            GUILayout.BeginArea(banner);
            GUILayout.BeginHorizontal();
            GUILayout.FlexibleSpace();
            GUILayout.Label("<size=18><b>⚡ QUANTUM TWO-BOX ECS ⚡</b></size>");
            GUILayout.FlexibleSpace();
            GUILayout.EndHorizontal();

            // Match timer display
            int remainingSecs = Mathf.Max(0, (int)_roundTimer.Remaining.AsFloat());
            string timerText = _roundTimer.HasExpired ? "<color=#FF4444><b>MATCH ENDED</b></color>" : $"⏱️ 00:{remainingSecs:D2}";

            GUILayout.BeginHorizontal();
            GUILayout.FlexibleSpace();
            GUILayout.Label($"<size=14><color=#44AAFF><b>🟦 P1 (WASD): {scoreP1}</b></color>   |   <color=#FFDD44><b>{timerText}</b></color>   |   <color=#FFAA33><b>🟧 P2 ({(p2IsBot ? "BOT" : "Arrows")}): {scoreP2}</b></color></size>");
            GUILayout.FlexibleSpace();
            GUILayout.EndHorizontal();
            GUILayout.EndArea();

            // Bot toggle button in bottom-left
            Rect botToggle = new Rect(20f, Screen.height - 50f, 240f, 32f);
            if (GUI.Button(botToggle, p2IsBot ? "🤖 P2: Bot AI (Click for Human)" : "🎮 P2: Human (Click for Bot)"))
            {
                p2IsBot = !p2IsBot;
            }

            // Desync Overlay Hint in top-right
            Rect f8Hint = new Rect(Screen.width - 200f, 20f, 180f, 28f);
            GUI.Box(f8Hint, GUIContent.none);
            GUI.Label(f8Hint, "  <color=#88FFAA><b>[F8] Desync Overlay</b></color>");

            // Reset Round button if expired
            if (_roundTimer.HasExpired)
            {
                Rect restartRect = new Rect((Screen.width - 160f) / 2f, 100f, 160f, 35f);
                if (GUI.Button(restartRect, "🔄 Play Again"))
                {
                    scoreP1 = 0;
                    scoreP2 = 0;
                    _roundTimer.Restart();
                }
            }
        }

        #region Auto-Boot Hook

        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.AfterSceneLoad)]
        private static void AutoBoot()
        {
#if UNITY_2023_1_OR_NEWER
            if (UnityEngine.Object.FindAnyObjectByType<CYGTwoBoxGame>() == null)
#else
            if (UnityEngine.Object.FindObjectOfType<CYGTwoBoxGame>() == null)
#endif
            {
                Debug.Log("<color=#44FF88><b>[CYG] Auto-Bootstrapping reference Two Box ECS Game into active scene!</b></color>");
                GameObject autoObj = new GameObject("[CYG Two Box reference AutoHost]");
                autoObj.AddComponent<CYGTwoBoxGame>();

                // Attach Desync Diagnostics Overlay (F8 toggle)
                if (autoObj.GetComponent<CYGDesyncOverlay>() == null)
                {
                    autoObj.AddComponent<CYGDesyncOverlay>();
                }

#if UNITY_2023_1_OR_NEWER
                if (UnityEngine.Object.FindAnyObjectByType<CYGDeterministicRunner>() == null)
#else
                if (UnityEngine.Object.FindObjectOfType<CYGDeterministicRunner>() == null)
#endif
                {
                    CYGDeterministicRunner runner = autoObj.AddComponent<CYGDeterministicRunner>();
                    runner.SimulationMode = CYGSimulationMode.Custom;
                    if (!runner.IsSimulationStarted)
                    {
                        runner.StartSimulation();
                    }
                }
            }
        }

        #endregion
    }
}
