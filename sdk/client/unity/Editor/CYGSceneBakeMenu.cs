using System;
using System.Collections.Generic;
using System.Text;
using CookYourGame.Sim;
using UnityEditor;
using UnityEngine;

namespace CookYourGame.EditorTools
{
    /// <summary>
    /// Editor-side helpers for the deterministic scene bake and static navmesh.
    /// </summary>
    public static class CYGSceneBakeMenu
    {
        [MenuItem("CookYourGame/Sim/Bake Nav Mesh Asset...")]
        public static void BakeNavMeshAsset()
        {
            CYGNavMeshAuthoring authoring = Selection.activeGameObject != null
                ? Selection.activeGameObject.GetComponent<CYGNavMeshAuthoring>()
                : null;

            if (authoring == null)
            {
                Debug.LogError("[CYG] Select a GameObject with a CYGNavMeshAuthoring component first.");
                return;
            }

            Vector3[] vertices;
            int[] indices;
            if (!authoring.TryBuildSource(out vertices, out indices) || vertices.Length == 0)
            {
                Debug.LogError("[CYG] No readable mesh geometry found under " + authoring.name + ".");
                return;
            }

            string path = EditorUtility.SaveFilePanelInProject(
                "Bake CYG Nav Mesh",
                authoring.name + "NavMesh",
                "asset",
                "Choose where to store the deterministic navmesh.");

            if (string.IsNullOrEmpty(path))
            {
                return;
            }

            CYGNavMeshAsset asset = AssetDatabase.LoadAssetAtPath<CYGNavMeshAsset>(path);
            if (asset == null)
            {
                asset = ScriptableObject.CreateInstance<CYGNavMeshAsset>();
                AssetDatabase.CreateAsset(asset, path);
            }

            asset.SetSource(vertices, indices);
            EditorUtility.SetDirty(asset);
            AssetDatabase.SaveAssets();
            AssetDatabase.Refresh();

            Debug.Log(
                $"[CYG] Baked nav mesh {asset.name}: vertices={vertices.Length} " +
                $"triangles={indices.Length / 3} checksum=0x{asset.BakedChecksum:X8}");

            Selection.activeObject = asset;
        }

        [MenuItem("CookYourGame/Sim/Validate Scene Bake")]
        public static void ValidateSceneBake()
        {
            CYGEntityAuthoring[] found = UnityEngine.Object.FindObjectsOfType<CYGEntityAuthoring>(true);
            List<CYGEntityAuthoring> ordered = new List<CYGEntityAuthoring>(found);
            ordered.Sort(CompareAuthoring);

            StringBuilder report = new StringBuilder();
            report.AppendLine("[CYG] Scene bake order (" + ordered.Count + " entities):");

            Dictionary<int, string> playerPaths = new Dictionary<int, string>();
            List<string> problems = new List<string>();
            int localCount = 0;

            for (int i = 0; i < ordered.Count; i++)
            {
                CYGEntityAuthoring entry = ordered[i];
                string path = PathOf(entry.transform);
                report.AppendLine($"  [{i}] order={entry.BakeOrder} player={entry.PlayerId} path={path}");

                if (entry.IsLocalPlayer)
                {
                    localCount++;
                }

                if (entry.PlayerId > 0)
                {
                    string existing;
                    if (playerPaths.TryGetValue(entry.PlayerId, out existing))
                    {
                        problems.Add($"Duplicate player id {entry.PlayerId}: \"{existing}\" and \"{path}\".");
                    }
                    else
                    {
                        playerPaths.Add(entry.PlayerId, path);
                    }
                }
            }

            if (localCount > 1)
            {
                problems.Add($"{localCount} entities are marked as the local player; at most one is allowed.");
            }

            if (problems.Count > 0)
            {
                for (int i = 0; i < problems.Count; i++)
                {
                    report.AppendLine("  ERROR: " + problems[i]);
                }
                Debug.LogError(report.ToString());
            }
            else
            {
                Debug.Log(report.ToString());
            }
        }

        private static int CompareAuthoring(CYGEntityAuthoring left, CYGEntityAuthoring right)
        {
            int order = left.BakeOrder.CompareTo(right.BakeOrder);
            if (order != 0)
            {
                return order;
            }
            return string.CompareOrdinal(PathOf(left.transform), PathOf(right.transform));
        }

        private static string PathOf(Transform transform)
        {
            string path = transform.name;
            Transform parent = transform.parent;
            while (parent != null)
            {
                path = parent.name + "/" + path;
                parent = parent.parent;
            }
            return path;
        }
    }
}
