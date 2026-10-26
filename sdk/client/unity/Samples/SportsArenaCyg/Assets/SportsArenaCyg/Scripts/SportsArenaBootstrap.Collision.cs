using System;
using System.Collections.Generic;
using CookYourGame.Sim;
using UnityEngine;

namespace CookYourGame.SportsArena
{
    public sealed partial class SportsArenaBootstrap
    {
        private static void StripCameras(GameObject root)
        {
            foreach (Camera camera in root.GetComponentsInChildren<Camera>(true))
            {
                camera.gameObject.SetActive(false);
            }
            foreach (AudioListener listener in root.GetComponentsInChildren<AudioListener>(true))
            {
                listener.enabled = false;
            }
        }

        private static Bounds ComputeBounds(GameObject root)
        {
            Collider[] colliders = root.GetComponentsInChildren<Collider>(true);
            bool has = false;
            Bounds bounds = new Bounds(Vector3.zero, new Vector3(30f, 8f, 30f));
            for (int i = 0; i < colliders.Length; i++)
            {
                Collider collider = colliders[i];
                if (collider == null || collider.isTrigger)
                {
                    continue;
                }
                if (!has)
                {
                    bounds = collider.bounds;
                    has = true;
                }
                else
                {
                    bounds.Encapsulate(collider.bounds);
                }
            }

            if (!has)
            {
                Renderer[] renderers = root.GetComponentsInChildren<Renderer>(true);
                for (int i = 0; i < renderers.Length; i++)
                {
                    if (!has)
                    {
                        bounds = renderers[i].bounds;
                        has = true;
                    }
                    else
                    {
                        bounds.Encapsulate(renderers[i].bounds);
                    }
                }
            }

            bounds.extents = new Vector3(
                Mathf.Clamp(bounds.extents.x, 8f, 100f),
                Mathf.Clamp(bounds.extents.y, 3f, 10f),
                Mathf.Clamp(bounds.extents.z, 8f, 100f));
            return bounds;
        }

        private void AddEnvironmentCollision(GameObject root, Bounds bounds)
        {
            int boxCount = 0;
            BoxCollider[] boxes = root.GetComponentsInChildren<BoxCollider>(true);
            for (int i = 0; i < boxes.Length; i++)
            {
                BoxCollider box = boxes[i];
                if (box.isTrigger || box.name.IndexOf("Goal", StringComparison.OrdinalIgnoreCase) >= 0)
                {
                    continue;
                }
                Vector3 worldCenter = box.transform.TransformPoint(box.center);
                Vector3 scale = box.transform.lossyScale;
                Vector3 half = new Vector3(
                    Mathf.Abs(box.size.x * scale.x) * 0.5f,
                    Mathf.Abs(box.size.y * scale.y) * 0.5f,
                    Mathf.Abs(box.size.z * scale.z) * 0.5f);
                if (half.x < 0.05f || half.y < 0.05f || half.z < 0.05f)
                {
                    continue;
                }
                Simulation.AddStaticBox(
                    SportsArenaConfig.ToFixed(worldCenter),
                    SportsArenaConfig.ToFixed(half),
                    SportsArenaConfig.ToFixed(box.transform.rotation));
                boxCount++;
            }

            int meshCount = 0;
            MeshCollider[] meshes = root.GetComponentsInChildren<MeshCollider>(true);
            for (int i = 0; i < meshes.Length; i++)
            {
                MeshCollider collider = meshes[i];
                if (collider.isTrigger || collider.sharedMesh == null || collider.convex)
                {
                    continue;
                }
                if (TryBakeMesh(collider, i))
                {
                    meshCount++;
                }
            }

            // The original sample stores its real level colliders in a baked
            // map asset, not in Unity colliders. Approximate the
            // visible arena blocks and walls so the port is immediately
            // playable with the original environment art.
            int rendererCount = 0;
            MeshRenderer[] renderers = root.GetComponentsInChildren<MeshRenderer>(true);
            for (int i = 0; i < renderers.Length; i++)
            {
                MeshRenderer renderer = renderers[i];
                if (!HasArenaCollisionName(renderer.transform))
                {
                    continue;
                }
                Bounds b = renderer.bounds;
                Vector3 size = b.size;
                if (size.x < 0.15f || size.y < 0.15f || size.z < 0.15f || b.center.y > 8f)
                {
                    continue;
                }
                Simulation.AddStaticBox(
                    SportsArenaConfig.ToFixed(b.center),
                    SportsArenaConfig.ToFixed(size * 0.5f),
                    FPQuaternion.Identity);
                rendererCount++;
            }

            // A deterministic floor fallback keeps the sample playable even if
            // the imported environment only ships decorative meshes.
            Simulation.AddStaticBox(
                SportsArenaConfig.ToFixed(new Vector3(bounds.center.x, -0.5f, bounds.center.z)),
                SportsArenaConfig.ToFixed(new Vector3(bounds.extents.x + 2f, 0.5f, bounds.extents.z + 2f)),
                FPQuaternion.Identity);

            Debug.Log($"[SportsArena] Static collision: {boxCount} boxes, {meshCount} baked meshes, {rendererCount} renderer boxes.");
        }

        private static bool HasArenaCollisionName(Transform transform)
        {
            for (int depth = 0; depth < 4 && transform != null; depth++)
            {
                string name = transform.name.ToLowerInvariant();
                if (name.IndexOf("team base", StringComparison.Ordinal) >= 0
                    || name.IndexOf("goal area", StringComparison.Ordinal) >= 0
                    || name == "gfx")
                {
                    return false;
                }
                if (name.IndexOf("block", StringComparison.Ordinal) >= 0)
                {
                    return true;
                }
                transform = transform.parent;
            }
            return false;
        }

        private bool TryBakeMesh(MeshCollider collider, int index)
        {
            try
            {
                Mesh source = collider.sharedMesh;
                if (!source.isReadable)
                {
                    return false;
                }
                Vector3[] local = source.vertices;
                int[] triangles = source.triangles;
                if (local.Length == 0 || triangles.Length == 0)
                {
                    return false;
                }
                Matrix4x4 matrix = collider.transform.localToWorldMatrix;
                FPVector3[] vertices = new FPVector3[local.Length];
                for (int i = 0; i < local.Length; i++)
                {
                    vertices[i] = SportsArenaConfig.ToFixed(matrix.MultiplyPoint3x4(local[i]));
                }
                string key = collider.transform.position.ToString("F2") + "/" + source.name + "/" + index;
                int id = CYGMeshRegistry.StableId(key);
                Simulation.AddStaticMesh(new CYGStaticMesh(id, key, vertices, triangles));
                return true;
            }
            catch (Exception exception)
            {
                Debug.LogWarning("[SportsArena] Mesh bake skipped: " + exception.Message);
                return false;
            }
        }

        private void AddArenaBounds(FPAABB arena)
        {
            FPVector3 center = arena.Center;
            FPVector3 extents = arena.Extents;
            FPVector3 wallCenter = new FPVector3(center.x, arena.Min.y + FP.FromInt(3), center.z);
            FPVector3 half = new FPVector3(FP.FromFraction(1, 2), FP.FromInt(3), extents.z + FP.One);
            Simulation.AddStaticBox(wallCenter + new FPVector3(-extents.x - FP.One, FP.Zero, FP.Zero), half, FPQuaternion.Identity);
            Simulation.AddStaticBox(wallCenter + new FPVector3(extents.x + FP.One, FP.Zero, FP.Zero), half, FPQuaternion.Identity);
            half = new FPVector3(extents.x + FP.One, FP.FromInt(3), FP.FromFraction(1, 2));
            Simulation.AddStaticBox(wallCenter + new FPVector3(FP.Zero, FP.Zero, -extents.z - FP.One), half, FPQuaternion.Identity);
            Simulation.AddStaticBox(wallCenter + new FPVector3(FP.Zero, FP.Zero, extents.z + FP.One), half, FPQuaternion.Identity);
        }
    }
}
