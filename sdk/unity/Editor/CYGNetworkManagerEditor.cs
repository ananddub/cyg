using UnityEditor;
using UnityEngine;

namespace CookYourGame.EditorTools
{
    [CustomEditor(typeof(CYGNetworkManager))]
    public sealed class CYGNetworkManagerEditor : Editor
    {
        SerializedProperty serverUrlProp;
        SerializedProperty roomNameProp;
        SerializedProperty settingsProp;
        SerializedProperty prefabsProp;
        SerializedProperty sendRateProp;
        SerializedProperty autoStartProp;
        SerializedProperty persistSingletonProp;
        SerializedProperty playerNameProp;
        SerializedProperty playerPrefabProp;
        SerializedProperty autoSpawnPlayerProp;
        SerializedProperty spawnRadiusProp;

        void OnEnable()
        {
            serverUrlProp = serializedObject.FindProperty("serverUrl");
            roomNameProp = serializedObject.FindProperty("roomName");
            settingsProp = serializedObject.FindProperty("settings");
            prefabsProp = serializedObject.FindProperty("prefabs");
            sendRateProp = serializedObject.FindProperty("sendRate");
            autoStartProp = serializedObject.FindProperty("autoStart");
            persistSingletonProp = serializedObject.FindProperty("persistSingleton");
            playerNameProp = serializedObject.FindProperty("playerName");
            playerPrefabProp = serializedObject.FindProperty("playerPrefab");
            autoSpawnPlayerProp = serializedObject.FindProperty("autoSpawnPlayer");
            spawnRadiusProp = serializedObject.FindProperty("spawnRadius");
        }

        public override bool RequiresConstantRepaint()
        {
            return Application.isPlaying;
        }

        public override void OnInspectorGUI()
        {
            serializedObject.Update();
            CYGNetworkManager manager = (CYGNetworkManager)target;

            DrawLiveStatusSection(manager);

            EditorGUILayout.Space(6);
            EditorGUILayout.LabelField("Server Connection", EditorStyles.boldLabel);
            EditorGUILayout.PropertyField(serverUrlProp, new GUIContent("Server URL", "Base HTTP/WS endpoint of the server (e.g. http://127.0.0.1:7777)"));
            EditorGUILayout.PropertyField(roomNameProp, new GUIContent("Room Name", "Room name to join or create (e.g. game_room)"));
            EditorGUILayout.PropertyField(settingsProp, new GUIContent("Settings (Optional)", "ScriptableObject asset for advanced settings. Overrides fields above if assigned."));

            EditorGUILayout.Space(6);
            EditorGUILayout.LabelField("Player Auto-Spawner", EditorStyles.boldLabel);
            EditorGUILayout.PropertyField(playerPrefabProp, new GUIContent("Player Prefab", "Prefab with CYGView and CYGTransformSync (e.g. Cube)"));
            EditorGUILayout.PropertyField(autoSpawnPlayerProp, new GUIContent("Auto Spawn on Join", "Automatically spawn playerPrefab when joining a room"));
            if (autoSpawnPlayerProp.boolValue)
            {
                EditorGUILayout.PropertyField(spawnRadiusProp, new GUIContent("Spawn Radius", "Random spread radius around spawn origin"));
            }

            EditorGUILayout.Space(6);
            EditorGUILayout.LabelField("Gameplay & Sync", EditorStyles.boldLabel);
            EditorGUILayout.PropertyField(prefabsProp, new GUIContent("Prefabs Registry", "Prefab registry asset for network instantiation"));
            EditorGUILayout.PropertyField(sendRateProp, new GUIContent("Send Rate (Hz)", "Network tick rate (1-60 Hz)"));
            EditorGUILayout.PropertyField(autoStartProp, new GUIContent("Auto Start", "Auto-connect policy on scene load"));
            EditorGUILayout.PropertyField(persistSingletonProp, new GUIContent("Dont Destroy On Load", "Keep NetworkManager across scene loads"));
            EditorGUILayout.PropertyField(playerNameProp, new GUIContent("Player Name", "Optional display name for local player"));

            serializedObject.ApplyModifiedProperties();
        }

        void DrawLiveStatusSection(CYGNetworkManager manager)
        {
            EditorGUILayout.Space(4);

            GUIStyle headerBox = new GUIStyle(EditorStyles.helpBox);
            headerBox.padding = new RectOffset(10, 10, 8, 8);

            EditorGUILayout.BeginVertical(headerBox);

            bool isConnected = CYGNetwork.IsConnected;
            bool isConnecting = CYGNetwork.IsConnecting;

            if (isConnected)
            {
                GUI.color = new Color(0.2f, 0.9f, 0.3f);
                EditorGUILayout.LabelField("● PROTOCOL STATUS: CONNECTED", EditorStyles.boldLabel);
                GUI.color = Color.white;

                EditorGUI.indentLevel++;
                EditorGUILayout.LabelField("Room", CYGNetwork.RoomName);
                EditorGUILayout.LabelField("Player ID", "#" + CYGNetwork.LocalActorNumber + (CYGNetwork.IsMasterClient ? " (Host / Master)" : " (Peer)"));
                EditorGUILayout.LabelField("Latency", CYGNetwork.Latency > 0 ? CYGNetwork.Latency.ToString("F0") + " ms" : "< 1 ms");
                EditorGUILayout.LabelField("Players in Room", CYGNetwork.Players != null ? CYGNetwork.Players.Count.ToString() : "1");
                EditorGUILayout.LabelField("Transport", CYGNetwork.TransportName);
                if (!string.IsNullOrEmpty(CYGNetwork.SessionId))
                {
                    EditorGUILayout.LabelField("Session ID", CYGNetwork.SessionId);
                }
                EditorGUI.indentLevel--;

                EditorGUILayout.Space(4);
                if (Application.isPlaying)
                {
                    GUI.backgroundColor = new Color(0.9f, 0.3f, 0.3f);
                    if (GUILayout.Button("Disconnect", GUILayout.Height(24)))
                    {
                        manager.Shutdown();
                    }
                    GUI.backgroundColor = Color.white;
                }
            }
            else if (isConnecting)
            {
                GUI.color = new Color(1f, 0.85f, 0.2f);
                EditorGUILayout.LabelField("◌ PROTOCOL STATUS: CONNECTING...", EditorStyles.boldLabel);
                GUI.color = Color.white;

                EditorGUI.indentLevel++;
                EditorGUILayout.LabelField("Target Server", string.IsNullOrEmpty(manager.ServerUrl) ? "Default" : manager.ServerUrl);
                EditorGUILayout.LabelField("Target Room", string.IsNullOrEmpty(manager.RoomName) ? "Default" : manager.RoomName);
                EditorGUI.indentLevel--;
            }
            else
            {
                GUI.color = new Color(0.7f, 0.7f, 0.7f);
                EditorGUILayout.LabelField("○ PROTOCOL STATUS: DISCONNECTED", EditorStyles.boldLabel);
                GUI.color = Color.white;

                if (Application.isPlaying)
                {
                    EditorGUILayout.Space(4);
                    GUI.backgroundColor = new Color(0.2f, 0.7f, 1f);
                    if (GUILayout.Button("Connect to Server", GUILayout.Height(26)))
                    {
                        manager.StartClient();
                    }
                    GUI.backgroundColor = Color.white;
                }
                else
                {
                    EditorGUILayout.HelpBox("Enter Play Mode to connect and view live protocol metrics.", MessageType.None);
                }
            }

            if (!string.IsNullOrEmpty(CYGNetwork.LastErrorMessage))
            {
                EditorGUILayout.Space(4);
                EditorGUILayout.HelpBox($"Last Error (Code {CYGNetwork.LastErrorCode}): {CYGNetwork.LastErrorMessage}", MessageType.Warning);
            }

            EditorGUILayout.EndVertical();
        }
    }
}
