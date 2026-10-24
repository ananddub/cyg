using System;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Bakes one or more Unity <see cref="MeshCollider"/>s into a deterministic
    /// <see cref="CYGStaticMesh"/> at startup. Level geometry (ramps, slopes,
    /// walls, tunnels) is converted to fixed-point world-space triangles and
    /// registered by a stable id, so every client queries the same mesh.
    ///
    /// The bake is static content. Move a baked mesh only by re-baking it.
    /// </summary>
    [DisallowMultipleComponent]
    [AddComponentMenu("CookYourGame/Sim/CYG Static Mesh Collider")]
    public sealed class CYGStaticMeshAuthoring : MonoBehaviour
    {
        [Tooltip("Stable mesh id. Zero derives an id from the hierarchy path.")]
        [SerializeField] private int meshId;

        [Tooltip("Colliders to bake. Empty collects MeshColliders on this object and its children.")]
        [SerializeField] private MeshCollider[] colliders;

        [Tooltip("Include inactive child colliders when the list is auto-collected.")]
        [SerializeField] private bool includeInactive = true;

        [Tooltip("Log a warning when a collider has no shared mesh.")]
        [SerializeField] private bool logMissingMeshes = true;

        public int MeshId => meshId != 0 ? meshId : CYGMeshRegistry.StableId(HierarchyPath(transform));

        public IReadOnlyList<MeshCollider> Colliders
        {
            get
            {
                EnsureColliders();
                return colliders;
            }
        }

        /// <summary>Bakes this authoring object and optionally registers the mesh.</summary>
        public CYGStaticMesh Bake(bool register = true)
        {
            EnsureColliders();

            List<Vector3> vertices = new List<Vector3>(1024);
            List<int> indices = new List<int>(3072);

            for (int i = 0; i < colliders.Length; i++)
            {
                MeshCollider collider = colliders[i];
                if (collider == null)
                {
                    continue;
                }

                Mesh mesh = collider.sharedMesh;
                if (mesh == null)
                {
                    if (logMissingMeshes)
                    {
                        Debug.LogWarning(
                            "[CYG] Static mesh collider on '" + collider.name + "' has no shared mesh; skipped.",
                            collider);
                    }
                    continue;
                }

                if (!mesh.isReadable)
                {
                    if (logMissingMeshes)
                    {
                        Debug.LogWarning(
                            "[CYG] Mesh '" + mesh.name + "' on '" + collider.name
                            + "' is not readable. Enable Read/Write in the model import settings to bake it.",
                            collider);
                    }
                    continue;
                }

                Matrix4x4 matrix = collider.transform.localToWorldMatrix;
                int baseIndex = vertices.Count;
                Vector3[] localVertices = mesh.vertices;
                for (int v = 0; v < localVertices.Length; v++)
                {
                    vertices.Add(matrix.MultiplyPoint3x4(localVertices[v]));
                }

                int[] triangles = mesh.triangles;
                for (int t = 0; t < triangles.Length; t++)
                {
                    indices.Add(baseIndex + triangles[t]);
                }
            }

            if (vertices.Count == 0 || indices.Count == 0)
            {
                return null;
            }

            FPVector3[] fixedVertices = new FPVector3[vertices.Count];
            for (int i = 0; i < vertices.Count; i++)
            {
                fixedVertices[i] = new FPVector3(
                    FP.FromFloat(vertices[i].x),
                    FP.FromFloat(vertices[i].y),
                    FP.FromFloat(vertices[i].z));
            }

            CYGStaticMesh baked = new CYGStaticMesh(MeshId, name, fixedVertices, indices.ToArray());
            if (register)
            {
                CYGMeshRegistry.Register(baked);
            }
            return baked;
        }

        /// <summary>Bakes this object and logs the resulting triangle count.</summary>
        [ContextMenu("Bake Static Mesh")]
        public void BakeFromContextMenu()
        {
            CYGStaticMesh baked = Bake();
            if (baked == null)
            {
                Debug.LogWarning("[CYG] No triangles were baked on '" + name + "'.", this);
                return;
            }
            Debug.Log("[CYG] Baked static mesh '" + baked.Name + "' id=" + baked.Id
                + " vertices=" + baked.VertexCount + " triangles=" + baked.TriangleCount, this);
        }

        private void EnsureColliders()
        {
            if (colliders != null && colliders.Length > 0)
            {
                return;
            }

            colliders = GetComponentsInChildren<MeshCollider>(includeInactive);
        }

        private static string HierarchyPath(Transform target)
        {
            string path = target.name;
            Transform parent = target.parent;
            while (parent != null)
            {
                path = parent.name + "/" + path;
                parent = parent.parent;
            }
            return path;
        }
    }

    /// <summary>
    /// Scene-level helper that bakes every <see cref="CYGStaticMeshAuthoring"/>
    /// under a root. Used by <see cref="CYGDeterministicRunner"/> and by custom
    /// bootstrap code that owns the collision world.
    /// </summary>
    public static class CYGStaticMeshBake
    {
        public static void BakeAll(
            Transform root,
            List<CYGStaticMesh> output,
            bool clearOutput = true,
            bool register = true)
        {
            if (output == null)
            {
                throw new ArgumentNullException(nameof(output));
            }

            if (clearOutput)
            {
                output.Clear();
            }

            CYGStaticMeshAuthoring[] authoring;
            if (root != null)
            {
                authoring = root.GetComponentsInChildren<CYGStaticMeshAuthoring>(true);
            }
            else
            {
#if UNITY_2023_1_OR_NEWER
                authoring = UnityEngine.Object.FindObjectsByType<CYGStaticMeshAuthoring>(
                    FindObjectsInactive.Include,
                    FindObjectsSortMode.None);
#else
                authoring = UnityEngine.Object.FindObjectsOfType<CYGStaticMeshAuthoring>(true);
#endif
            }

            for (int i = 0; i < authoring.Length; i++)
            {
                if (authoring[i] == null)
                {
                    continue;
                }

                CYGStaticMesh mesh = authoring[i].Bake(register);
                if (mesh != null)
                {
                    output.Add(mesh);
                }
            }
        }

        /// <summary>Adds baked meshes to a character-controller collision world.</summary>
        public static void Populate(SimpleCollisionWorld world, IList<CYGStaticMesh> meshes)
        {
            if (world == null || meshes == null)
            {
                return;
            }

            for (int i = 0; i < meshes.Count; i++)
            {
                world.AddMesh(meshes[i]);
            }
        }
    }
}
