using System;
using UnityEngine;

namespace CookYourGame
{
    /// <summary>Drives the SDK: owns settings, the prefab registry and the network tick.</summary>
    [DisallowMultipleComponent]
    [DefaultExecutionOrder(-999)]
    public class CYGNetworkManager : MonoBehaviour
    {
        /// <summary>What the manager does when the scene loads.</summary>
        public enum StartFlags
        {
            /// <summary>Wait for an explicit StartClient call.</summary>
            None = 0,

            /// <summary>Connect with the inspector room name while playing in the editor.</summary>
            Editor = 1,

            /// <summary>Connect in a player build.</summary>
            ClientBuild = 2,

            /// <summary>Headless server build: the SDK stays a client of the room server.</summary>
            ServerBuild = 3,
        }

        const int MinSendRate = 1;
        const int MaxSendRate = 60;
        const int DefaultSendRate = 20;
        const int CatchUpLimit = 5;

        [Header("Server Connection")]
        [Tooltip("Base HTTP/WS endpoint of the game server, e.g. http://127.0.0.1:7777")]
        [SerializeField] string serverUrl = "http://127.0.0.1:7777";

        [Tooltip("Room name to join, e.g. game_room")]
        [SerializeField] string roomName = "game_room";

        [Tooltip("Optional custom settings ScriptableObject. If set, overrides the URL and Room above.")]
        [SerializeField] CYGServerSettings settings;

        [Header("Player Spawner")]
        [Tooltip("Prefab to instantiate when joining a room")]
        [SerializeField] GameObject playerPrefab;
        [Tooltip("Automatically spawn player prefab on room join")]
        [SerializeField] bool autoSpawnPlayer = true;
        [Tooltip("Spawn radius spread around (0, 0.6, 0)")]
        [SerializeField] float spawnRadius = 3f;

        [Header("Gameplay & Sync")]
        [SerializeField] CYGPrefabRegistry prefabs;
        [SerializeField] int sendRate = DefaultSendRate;
        [SerializeField] StartFlags autoStart = StartFlags.None;
        [SerializeField] bool persistSingleton = true;
        [SerializeField] string playerName = string.Empty;

        static CYGNetworkManager instance;
        static CYGServerSettings fallbackSettings;
        static CYGPrefabRegistry fallbackPrefabs;

        double tickAccumulator;
        double tickInterval = 1d / DefaultSendRate;

        /// <summary>The manager that currently drives the SDK, or null.</summary>
        public static CYGNetworkManager Instance
        {
            get { return instance; }
        }

        /// <summary>Configured server HTTP/WS endpoint.</summary>
        public string ServerUrl
        {
            get { return serverUrl; }
            set { serverUrl = value; }
        }

        /// <summary>Configured room name.</summary>
        public string RoomName
        {
            get { return roomName; }
            set { roomName = value; }
        }

        /// <summary>Fires at sendRate Hz while connected; run fixed simulation and sync here.</summary>
        public static event Action OnFixedNetworkTick;

        /// <summary>Fires every frame while connected; run interpolation here.</summary>
        public static event Action OnVariableUpdate;

        /// <summary>Settings the facade should use, creating a default instance when needed.</summary>
        public static CYGServerSettings CurrentSettings
        {
            get
            {
                if (instance != null)
                {
                    if (instance.settings != null)
                    {
                        return instance.settings;
                    }

                    if (fallbackSettings == null)
                    {
                        fallbackSettings = ScriptableObject.CreateInstance<CYGServerSettings>();
                        fallbackSettings.name = "CYGServerSettings (Inspector Config)";
                    }

                    fallbackSettings.ServerHttpUrl = string.IsNullOrEmpty(instance.serverUrl) ? "http://127.0.0.1:7777" : instance.serverUrl;
                    fallbackSettings.RoomName = string.IsNullOrEmpty(instance.roomName) ? "game_room" : instance.roomName;
                    return fallbackSettings;
                }

                return fallbackSettings != null ? fallbackSettings : EnsureFallbackSettings();
            }
        }

        /// <summary>Prefab registry the facade should use, creating a default instance when needed.</summary>
        public static CYGPrefabRegistry CurrentPrefabs
        {
            get
            {
                if (instance != null && instance.prefabs != null)
                {
                    return instance.prefabs;
                }

                return fallbackPrefabs != null ? fallbackPrefabs : EnsureFallbackPrefabs();
            }
        }

        /// <summary>Configured sync rate in hertz.</summary>
        public int SendRate
        {
            get { return sendRate; }
            set
            {
                sendRate = Mathf.Clamp(value, MinSendRate, MaxSendRate);
                tickInterval = 1d / sendRate;
            }
        }

        void Awake()
        {
            if (instance != null && instance != this)
            {
                Debug.LogWarning("[CYG] a second CYGNetworkManager was found; the first one keeps control.");
                Destroy(gameObject);
                return;
            }

            instance = this;
            Application.runInBackground = true;
            if (sendRate < 30) sendRate = 50;
            tickInterval = 1d / Mathf.Clamp(sendRate, MinSendRate, MaxSendRate);
            if (CurrentSettings != null && !string.IsNullOrEmpty(CurrentSettings.ServerHttpUrl))
            {
                CYGHttp.BaseUrl = CurrentSettings.ServerHttpUrl;
            }
            if (persistSingleton)
            {
                DontDestroyOnLoad(gameObject);
            }
            if (playerPrefab != null)
            {
                CurrentPrefabs.RegisterPrefab(playerPrefab);
            }
        }

        /// <summary>Configured player prefab to auto-spawn.</summary>
        public GameObject PlayerPrefab
        {
            get { return playerPrefab; }
            set { playerPrefab = value; }
        }

        /// <summary>Whether to automatically spawn playerPrefab on room join.</summary>
        public bool AutoSpawnPlayer
        {
            get { return autoSpawnPlayer; }
            set { autoSpawnPlayer = value; }
        }

        /// <summary>Spawn spread radius.</summary>
        public float SpawnRadius
        {
            get { return spawnRadius; }
            set { spawnRadius = value; }
        }

        void OnEnable()
        {
            instance = this;
            CYGNetwork.OnJoinedRoom += HandleRoomJoined;
        }

        void OnDisable()
        {
            CYGNetwork.OnJoinedRoom -= HandleRoomJoined;
            CYGNetwork.Disconnect();
        }

        void HandleRoomJoined(string room)
        {
            if (autoSpawnPlayer && playerPrefab != null)
            {
                SpawnLocalPlayer();
            }
        }

        /// <summary>Instantiates the configured playerPrefab over the network.</summary>
        public CYGView SpawnLocalPlayer()
        {
            if (playerPrefab == null) return null;

            CYGPrefabRegistry reg = CurrentPrefabs;
            if (reg != null)
            {
                reg.RegisterPrefab(playerPrefab);
            }

            Vector3 spawnPos = new Vector3(
                UnityEngine.Random.Range(-spawnRadius, spawnRadius),
                0.6f,
                UnityEngine.Random.Range(-spawnRadius, spawnRadius)
            );

            return CYGNetwork.SpawnPrefab(playerPrefab.name, spawnPos, Quaternion.identity);
        }

        void Update()
        {
            CYGNetwork.PumpInbound();

            if (!CYGNetwork.IsConnected)
            {
                tickAccumulator = 0;
                return;
            }

            double elapsed = Time.deltaTime;
            tickAccumulator += elapsed;

            int guard = 0;
            while (tickAccumulator >= tickInterval && guard < CatchUpLimit)
            {
                tickAccumulator -= tickInterval;
                guard++;
                if (OnFixedNetworkTick != null)
                {
                    OnFixedNetworkTick.Invoke();
                }
            }

            if (tickAccumulator > tickInterval * CatchUpLimit)
            {
                // A stalled frame must not turn into a burst of stale ticks.
                tickAccumulator = 0;
            }

            if (OnVariableUpdate != null)
            {
                OnVariableUpdate.Invoke();
            }
        }


        void OnDestroy()
        {
            if (instance == this)
            {
                CYGNetwork.Disconnect();
                instance = null;
            }
        }

        void OnApplicationQuit()
        {
            CYGNetwork.Disconnect();
        }

        void OnValidate()
        {
            if (sendRate < MinSendRate)
            {
                sendRate = MinSendRate;
            }

            if (sendRate > MaxSendRate)
            {
                sendRate = MaxSendRate;
            }

            tickInterval = 1d / sendRate;
        }

        /// <summary>Joins the configured room.</summary>
        public void StartClient()
        {
            StartClient(null);
        }

        /// <summary>Joins a room by name, defaults taken from the settings asset.</summary>
        public void StartClient(string roomName)
        {
            CYGServerSettings active = CurrentSettings;
            string room = string.IsNullOrEmpty(roomName) ? active.RoomName : roomName;
            CYGNetwork.ConnectToRoom(room, playerName);
        }

        /// <summary>Leaves the room and closes the link.</summary>
        public void Shutdown()
        {
            CYGNetwork.Disconnect();
        }

        /// <summary>Assigns the settings asset at runtime.</summary>
        public void SetSettings(CYGServerSettings value)
        {
            settings = value;
        }

        /// <summary>Assigns the prefab registry at runtime.</summary>
        public void SetPrefabs(CYGPrefabRegistry value)
        {
            prefabs = value;
        }

        void Start()
        {
            bool editorSession = Application.isEditor && autoStart == StartFlags.Editor;
            bool buildSession = !Application.isEditor && (autoStart == StartFlags.ClientBuild || autoStart == StartFlags.ServerBuild);
            if (editorSession || buildSession)
            {
                StartClient();
            }
        }

        static CYGServerSettings EnsureFallbackSettings()
        {
            fallbackSettings = ScriptableObject.CreateInstance<CYGServerSettings>();
            fallbackSettings.name = "CYGServerSettings (runtime default)";
            if (instance != null)
            {
                fallbackSettings.ServerHttpUrl = string.IsNullOrEmpty(instance.serverUrl) ? "http://127.0.0.1:7777" : instance.serverUrl;
                fallbackSettings.RoomName = string.IsNullOrEmpty(instance.roomName) ? "game_room" : instance.roomName;
            }
            return fallbackSettings;
        }

        static CYGPrefabRegistry EnsureFallbackPrefabs()
        {
            fallbackPrefabs = ScriptableObject.CreateInstance<CYGPrefabRegistry>();
            fallbackPrefabs.name = "CYGPrefabRegistry (runtime default)";
            return fallbackPrefabs;
        }

        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.SubsystemRegistration)]
        static void ResetDomain()
        {
            instance = null;
            fallbackSettings = null;
            fallbackPrefabs = null;
            OnFixedNetworkTick = null;
            OnVariableUpdate = null;
        }
    }
}
