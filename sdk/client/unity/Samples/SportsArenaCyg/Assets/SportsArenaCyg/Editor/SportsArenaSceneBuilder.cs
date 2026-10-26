using System.IO;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;

namespace CookYourGame.SportsArena.Editor
{
    /// <summary>Creates the small host scene used by the CYG-native port.</summary>
    public static class SportsArenaSceneBuilder
    {
        private const string ScenePath = "Assets/SportsArenaCyg/Scenes/SportsArena.unity";

        [MenuItem("CookYourGame/Sports Arena/Create Play Scene")]
        public static void Create()
        {
            Directory.CreateDirectory(Path.GetDirectoryName(ScenePath) ?? "Assets");
            Scene scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
            GameObject host = new GameObject("Sports Arena");
            host.AddComponent<SportsArenaBootstrap>();
            EditorSceneManager.MarkSceneDirty(scene);
            EditorSceneManager.SaveScene(scene, ScenePath);
            EditorBuildSettings.scenes = new[]
            {
                new EditorBuildSettingsScene(ScenePath, true),
            };
            AssetDatabase.SaveAssets();
            Debug.Log("[SportsArena] Created " + ScenePath);
        }

        public static void CreateBatch()
        {
            Create();
        }

        [MenuItem("CookYourGame/Sports Arena/Validate Art")]
        public static void ValidateArt()
        {
            string[] names = { "Environment", "Player", "Ball" };
            for (int i = 0; i < names.Length; i++)
            {
                GameObject prefab = Resources.Load<GameObject>(names[i]);
                if (prefab == null)
                {
                    Debug.LogError("[SportsArena] Missing Resources prefab: " + names[i]);
                    continue;
                }
                Renderer[] renderers = prefab.GetComponentsInChildren<Renderer>(true);
                int bad = 0;
                for (int r = 0; r < renderers.Length; r++)
                {
                    Material material = renderers[r].sharedMaterial;
                    if (material == null || material.shader == null
                        || material.shader.name.IndexOf("InternalError", System.StringComparison.Ordinal) >= 0)
                    {
                        bad++;
                    }
                }
                Debug.Log($"[SportsArena] {names[i]}: renderers={renderers.Length}, badMaterials={bad}, colliders={prefab.GetComponentsInChildren<Collider>(true).Length}");
            }
        }

        public static void ValidateArtBatch()
        {
            ValidateArt();
        }
    }
}
