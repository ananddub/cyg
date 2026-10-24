using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;
using CookYourGame.Sim;

namespace CookYourGame.EditorTools
{
    [InitializeOnLoad]
    public static class CYGTwoBoxGameMenu
    {
        static CYGTwoBoxGameMenu()
        {
            // Automatically refresh AssetDatabase so Unity picks up any new Sim ECS components
            AssetDatabase.Refresh();
        }

        [MenuItem("CookYourGame/Setup Two Box reference Scene")]
        public static void CreateTwoBoxScene()
        {
            // 1. Create a brand new clean scene
            Scene scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);

            // 2. Add Top-Down Main Camera (85 deg pitch, Y=16)
            GameObject camObj = new GameObject("Main Camera");
            camObj.tag = "MainCamera";
            Camera cam = camObj.AddComponent<Camera>();
            camObj.AddComponent<AudioListener>();
            camObj.transform.position = new Vector3(0, 16f, -1f);
            camObj.transform.rotation = Quaternion.Euler(85f, 0, 0);
            cam.clearFlags = CameraClearFlags.SolidColor;
            cam.backgroundColor = new Color(0.08f, 0.09f, 0.12f); // Deep dark space blue

            // 3. Add Directional Light
            GameObject lightObj = new GameObject("Directional Light");
            Light light = lightObj.AddComponent<Light>();
            light.type = LightType.Directional;
            light.color = Color.white;
            light.intensity = 1.2f;
            lightObj.transform.rotation = Quaternion.Euler(50f, -30f, 0);

            // 4. Add the Two Box Game Manager
            GameObject gameObj = new GameObject("[Two Box reference Game]");
            var runner = gameObj.AddComponent<CYGDeterministicRunner>();
            runner.SimulationMode = CYGSimulationMode.Custom;
            gameObj.AddComponent<CYGTwoBoxGame>();

            // 5. Ensure folder exists and save scene
            string scenePath = "Assets/Scenes/TwoBoxreferenceGame.unity";
            if (!AssetDatabase.IsValidFolder("Assets/Scenes"))
            {
                AssetDatabase.CreateFolder("Assets", "Scenes");
            }

            EditorSceneManager.SaveScene(scene, scenePath);
            AssetDatabase.Refresh();

            Debug.Log($"<color=#44FF88><b>[CYG] Two Box reference Scene successfully created and saved at {scenePath}!</b></color>\nPress <b>PLAY</b> in Unity to enjoy the deterministic two-box game!");
        }

        [MenuItem("CookYourGame/Add Two Box Game to Current Scene")]
        public static void AddTwoBoxToCurrentScene()
        {
#if UNITY_2022_2_OR_NEWER
            if (Object.FindAnyObjectByType<CYGTwoBoxGame>() != null)
#else
            if (Object.FindObjectOfType<CYGTwoBoxGame>() != null)
#endif
            {
                Debug.LogWarning("[CYG] CYGTwoBoxGame already exists in the current scene!");
                return;
            }

            GameObject gameObj = new GameObject("[Two Box reference Game]");
            var runner = gameObj.AddComponent<CYGDeterministicRunner>();
            runner.SimulationMode = CYGSimulationMode.Custom;
            gameObj.AddComponent<CYGTwoBoxGame>();

            EditorSceneManager.MarkSceneDirty(SceneManager.GetActiveScene());
            Debug.Log("<color=#44FF88><b>[CYG] Added [Two Box reference Game] to the current active scene! Press PLAY to start!</b></color>");
        }
    }
}
