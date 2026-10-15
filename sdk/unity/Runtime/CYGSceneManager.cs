using System;
using UnityEngine;
using UnityEngine.SceneManagement;

namespace CookYourGame
{
    /// <summary>Keeps every client's networked gameplay scene in sync via room events.</summary>
    public static class CYGSceneManager
    {
        /// <summary>Room-event name carrying scene changes.</summary>
        public const string SceneEventName = "CYG_SCENE";

        /// <summary>Raised after a networked scene finishes loading locally.</summary>
        public static event Action<string> OnNetworkSceneLoaded;

        static string currentNetworkScene;
        static bool loadInFlight;
        static CYGBitWriter s_writer = new CYGBitWriter();
        static CYGBitReader s_reader = new CYGBitReader();

        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.BeforeSceneLoad)]
        static void Boot()
        {
            currentNetworkScene = null;
            loadInFlight = false;
            CYGNetwork.OnRoomEvent -= HandleRoomEvent;
            CYGNetwork.OnRoomEvent += HandleRoomEvent;
            SceneManager.sceneLoaded -= HandleSceneLoaded;
            SceneManager.sceneLoaded += HandleSceneLoaded;
        }

        /// <summary>The last scene loaded or joined through the network, or null.</summary>
        public static string CurrentNetworkScene => currentNetworkScene;

        /// <summary>Master client loads a scene locally and tells every peer to load it too.</summary>
        public static void LoadNetworkedScene(string sceneName, LoadSceneMode mode = LoadSceneMode.Single)
        {
            if (string.IsNullOrEmpty(sceneName))
            {
                Debug.LogError("CYGSceneManager.LoadNetworkedScene: scene name is empty.");
                return;
            }

            if (!CYGNetwork.IsConnected)
            {
                Debug.LogError("CYGSceneManager.LoadNetworkedScene: not connected.");
                return;
            }

            if (!CYGNetwork.IsMasterClient)
            {
                Debug.LogError("CYGSceneManager.LoadNetworkedScene: only the master client may load a networked scene.");
                return;
            }

            // WHY: the event name is the first string block so the core can split it out before
            // handing the remainder to OnRoomEvent listeners as (eventName, data).
            s_writer.Reset();
            s_writer.WriteString(SceneEventName);
            s_writer.WriteString(sceneName);
            s_writer.WriteByte((byte)mode);
            CYGNetwork.SendFrame(CYGOpCode.ProtoDataBytes, s_writer.ToArraySegment());

            StartLoad(sceneName, mode);
        }

        static void HandleRoomEvent(string eventName, ArraySegment<byte> data)
        {
            if (eventName != SceneEventName || data.Array == null || data.Count == 0)
            {
                return;
            }

            s_reader.Attach(data.Array, data.Offset, data.Count);
            string sceneName = s_reader.ReadString();
            byte modeByte = s_reader.ReadByte();
            LoadSceneMode mode = modeByte == 1 ? LoadSceneMode.Additive : LoadSceneMode.Single;

            StartLoad(sceneName, mode);
        }

        static void StartLoad(string sceneName, LoadSceneMode mode)
        {
            if (string.IsNullOrEmpty(sceneName))
            {
                return;
            }

            // WHY: a master client hears its own CYG_SCENE broadcast, so ignore a load for the
            // scene whose local load is already in flight; Single reloads arrive after completion.
            if (loadInFlight && sceneName == currentNetworkScene)
            {
                return;
            }

            if (mode == LoadSceneMode.Additive && IsSceneLoaded(sceneName))
            {
                currentNetworkScene = sceneName;
                return;
            }

            currentNetworkScene = sceneName;
            loadInFlight = true;
            SceneManager.LoadScene(sceneName, mode);
        }

        static bool IsSceneLoaded(string sceneName)
        {
            Scene scene = SceneManager.GetSceneByName(sceneName);
            return scene.IsValid() && scene.isLoaded;
        }

        static void HandleSceneLoaded(string sceneName, LoadSceneMode mode)
        {
            loadInFlight = false;
            if (sceneName == currentNetworkScene)
            {
                OnNetworkSceneLoaded?.Invoke(sceneName);
            }
        }
    }
}
