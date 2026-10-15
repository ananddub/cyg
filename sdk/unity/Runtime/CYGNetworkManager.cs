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

        [SerializeField] CYGServerSettings settings;
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

        /// <summary>Fires at sendRate Hz while connected; run fixed simulation and sync here.</summary>
        public static event Action OnFixedNetworkTick;

        /// <summary>Fires every frame while connected; run interpolation here.</summary>
        public static event Action OnVariableUpdate;

        /// <summary>Settings the facade should use, creating a default instance when needed.</summary>
        public static CYGServerSettings CurrentSettings
        {
            get
            {
                if (instance != null && instance.settings != null)
                {
                    return instance.settings;
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
            tickInterval = 1d / Mathf.Clamp(sendRate, MinSendRate, MaxSendRate);
            if (persistSingleton)
            {
                DontDestroyOnLoad(gameObject);
            }
        }

        void OnEnable()
        {
            instance = this;
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
            Debug.LogWarning("[CYG] no CYGServerSettings asset was assigned; using runtime defaults. Create Assets/CYGServerSettings.asset to configure the server url.");
            return fallbackSettings;
        }

        static CYGPrefabRegistry EnsureFallbackPrefabs()
        {
            fallbackPrefabs = ScriptableObject.CreateInstance<CYGPrefabRegistry>();
            fallbackPrefabs.name = "CYGPrefabRegistry (runtime default)";
            return fallbackPrefabs;
        }
    }
}
