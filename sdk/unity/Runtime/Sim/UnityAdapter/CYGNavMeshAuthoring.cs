using System;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Serializable navmesh bake. Every client must load the same asset; the
    /// mesh is static level data and is deliberately not part of the world
    /// snapshot.
    /// </summary>
    [CreateAssetMenu(
        fileName = "CYGNavMesh",
        menuName = "CookYourGame/Sim/Nav Mesh Asset",
        order = 100)]
    public sealed class CYGNavMeshAsset : ScriptableObject
    {
        [SerializeField] private Vector3[] sourceVertices = Array.Empty<Vector3>();
        [SerializeField] private int[] sourceIndices = Array.Empty<int>();
        [SerializeField] private float slopeLimitDegrees = 45f;
        [SerializeField] private float agentRadius = 0.5f;
        [SerializeField] private float agentHeight = 1.8f;

        [SerializeField, HideInInspector] private uint bakedChecksum;

        private CYGNavMesh _runtime;

        public uint BakedChecksum => bakedChecksum;
        public int VertexCount => sourceVertices.Length;
        public int TriangleCount => sourceIndices.Length / 3;

        public void SetSource(Vector3[] vertices, int[] indices)
        {
            if (vertices == null) throw new ArgumentNullException(nameof(vertices));
            if (indices == null) throw new ArgumentNullException(nameof(indices));
            if ((indices.Length % 3) != 0)
            {
                throw new ArgumentException("Index count must be a multiple of three.", nameof(indices));
            }

            sourceVertices = vertices;
            sourceIndices = indices;
            _runtime = null;
            bakedChecksum = Build().Checksum;
        }

        public CYGNavMesh Build()
        {
            if (_runtime != null)
            {
                return _runtime;
            }

            if (sourceVertices.Length == 0 || sourceIndices.Length == 0)
            {
                throw new InvalidOperationException(
                    "Nav mesh asset \"" + name + "\" has no baked geometry. Bake it from a CYGNavMeshAuthoring component.");
            }

            FPVector3[] vertices = new FPVector3[sourceVertices.Length];
            for (int i = 0; i < sourceVertices.Length; i++)
            {
                Vector3 value = sourceVertices[i];
                vertices[i] = new FPVector3(
                    FP.FromFloat(value.x),
                    FP.FromFloat(value.y),
                    FP.FromFloat(value.z));
            }

            CYGNavBuildConfig config = new CYGNavBuildConfig
            {
                SlopeLimitDegrees = FP.FromFloat(slopeLimitDegrees),
                AgentRadius = FP.FromFloat(agentRadius),
                AgentHeight = FP.FromFloat(agentHeight),
            };

            _runtime = CYGNavMeshBuilder.Build(vertices, sourceIndices, config);
            bakedChecksum = _runtime.Checksum;
            return _runtime;
        }

        private void OnValidate()
        {
            _runtime = null;
        }
    }

    /// <summary>
    /// Bakes a navmesh from the MeshFilter triangles below this object.
    /// Runtime baking is supported for prototypes; production scenes should
    /// bake to a <see cref="CYGNavMeshAsset"/> so all clients share bytes.
    /// </summary>
    [DisallowMultipleComponent]
    [AddComponentMenu("CookYourGame/Sim/CYG Nav Mesh Authoring")]
    public sealed class CYGNavMeshAuthoring : MonoBehaviour
    {
        [SerializeField] private MeshFilter[] sourceMeshes;
        [SerializeField] private float slopeLimitDegrees = 45f;
        [SerializeField] private float agentRadius = 0.5f;
        [SerializeField] private float agentHeight = 1.8f;

        public CYGNavMesh BuildNavMesh()
        {
            Vector3[] vertices;
            int[] indices;
            if (!TryBuildSource(out vertices, out indices) || vertices.Length == 0)
            {
                throw new InvalidOperationException(
                    "No readable MeshFilter geometry found under \"" + name + "\".");
            }

            FPVector3[] fixedVertices = new FPVector3[vertices.Length];
            for (int i = 0; i < vertices.Length; i++)
            {
                fixedVertices[i] = new FPVector3(
                    FP.FromFloat(vertices[i].x),
                    FP.FromFloat(vertices[i].y),
                    FP.FromFloat(vertices[i].z));
            }

            CYGNavBuildConfig config = new CYGNavBuildConfig
            {
                SlopeLimitDegrees = FP.FromFloat(slopeLimitDegrees),
                AgentRadius = FP.FromFloat(agentRadius),
                AgentHeight = FP.FromFloat(agentHeight),
            };

            return CYGNavMeshBuilder.Build(fixedVertices, indices, config);
        }

        /// <summary>Collects world-space triangle soup for an asset bake.</summary>
        public bool TryBuildSource(out Vector3[] vertices, out int[] indices)
        {
            List<Vector3> vertexList = new List<Vector3>();
            List<int> indexList = new List<int>();

            MeshFilter[] meshes = sourceMeshes;
            if (meshes == null || meshes.Length == 0)
            {
                meshes = GetComponentsInChildren<MeshFilter>(true);
            }

            for (int m = 0; m < meshes.Length; m++)
            {
                MeshFilter filter = meshes[m];
                if (filter == null || filter.sharedMesh == null)
                {
                    continue;
                }

                if (!filter.sharedMesh.isReadable)
                {
                    Debug.LogError(
                        "[CYG] Navmesh mesh \"" + filter.name + "\" is not readable. Enable Read/Write or bake from a CYGNavMeshAsset.",
                        filter);
                    continue;
                }

                int baseIndex = vertexList.Count;
                Matrix4x4 matrix = filter.transform.localToWorldMatrix;
                Vector3[] meshVertices = filter.sharedMesh.vertices;
                int[] meshIndices = filter.sharedMesh.triangles;

                for (int i = 0; i < meshVertices.Length; i++)
                {
                    vertexList.Add(matrix.MultiplyPoint3x4(meshVertices[i]));
                }
                for (int i = 0; i < meshIndices.Length; i++)
                {
                    indexList.Add(baseIndex + meshIndices[i]);
                }
            }

            vertices = vertexList.ToArray();
            indices = indexList.ToArray();
            return vertices.Length > 0;
        }
    }
}
