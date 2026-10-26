using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Immutable deterministic triangle mesh used for static level collision
    /// (ramps, slopes, walls and arbitrary level geometry). Vertices are
    /// already in world space and stored as fixed-point values, so the mesh is
    /// identical on every client and can be queried without UnityEngine.
    ///
    /// Meshes are static content: they are registered once in
    /// <see cref="CYGMeshRegistry"/> and are not part of world snapshots.
    /// </summary>
    public sealed class CYGStaticMesh
    {
        private readonly FPVector3[] _vertices;
        private readonly int[] _indices;
        private readonly FPAABB[] _triangleBounds;
        private FPAABB _bounds;

        public CYGStaticMesh(int id, string name, FPVector3[] vertices, int[] indices)
        {
            if (vertices == null)
            {
                throw new ArgumentNullException(nameof(vertices));
            }
            if (indices == null)
            {
                throw new ArgumentNullException(nameof(indices));
            }
            if (indices.Length == 0 || indices.Length % 3 != 0)
            {
                throw new ArgumentException("Mesh indices must contain a non-zero multiple of three entries.", nameof(indices));
            }

            Id = id;
            Name = string.IsNullOrEmpty(name) ? "Mesh" + id : name;
            _vertices = (FPVector3[])vertices.Clone();
            _indices = (int[])indices.Clone();
            _triangleBounds = new FPAABB[_indices.Length / 3];

            FPVector3 min = new FPVector3(FP.MaxValue, FP.MaxValue, FP.MaxValue);
            FPVector3 max = new FPVector3(FP.MinValue, FP.MinValue, FP.MinValue);
            bool any = false;

            for (int i = 0; i < _indices.Length; i++)
            {
                int index = _indices[i];
                if (index < 0 || index >= _vertices.Length)
                {
                    throw new ArgumentException("Mesh index " + index + " is outside the vertex array.", nameof(indices));
                }
                FPVector3 vertex = _vertices[index];
                min = FPVector3.Min(min, vertex);
                max = FPVector3.Max(max, vertex);
                any = true;
            }

            for (int t = 0; t < _triangleBounds.Length; t++)
            {
                FPVector3 a = _vertices[_indices[t * 3]];
                FPVector3 b = _vertices[_indices[t * 3 + 1]];
                FPVector3 c = _vertices[_indices[t * 3 + 2]];
                _triangleBounds[t] = new FPAABB(
                    FPVector3.Min(a, FPVector3.Min(b, c)),
                    FPVector3.Max(a, FPVector3.Max(b, c)));
            }

            _bounds = any ? new FPAABB(min, max) : new FPAABB(FPVector3.Zero, FPVector3.Zero);
        }

        public int Id { get; }

        public string Name { get; }

        public int VertexCount
        {
            get { return _vertices.Length; }
        }

        public int TriangleCount
        {
            get { return _triangleBounds.Length; }
        }

        public FPAABB Bounds
        {
            get { return _bounds; }
        }

        public FPVector3 GetVertex(int index)
        {
            return _vertices[index];
        }

        public void GetTriangle(int triangle, out FPVector3 a, out FPVector3 b, out FPVector3 c)
        {
            if ((uint)triangle >= (uint)_triangleBounds.Length)
            {
                throw new ArgumentOutOfRangeException(nameof(triangle));
            }
            int baseIndex = triangle * 3;
            a = _vertices[_indices[baseIndex]];
            b = _vertices[_indices[baseIndex + 1]];
            c = _vertices[_indices[baseIndex + 2]];
        }

        public bool Raycast(FPRay ray, FP maxDistance, out FPRaycastHit hit)
        {
            return CYGMeshQueries.Raycast(this, ray, maxDistance, out hit);
        }

        public bool SphereCast(FPSphere sphere, FPVector3 direction, FP maxDistance, out FPRaycastHit hit)
        {
            return CYGMeshQueries.SphereCast(this, sphere, direction, maxDistance, out hit);
        }

        public bool CapsuleCast(FPCapsule capsule, FPVector3 direction, FP maxDistance, out FPRaycastHit hit)
        {
            return CYGMeshCasts.CapsuleCast(this, capsule, direction, maxDistance, out hit);
        }

        public bool Overlaps(FPSphere sphere)
        {
            return CYGMeshCasts.Overlaps(this, sphere);
        }

        public bool Overlaps(FPCapsule capsule)
        {
            return CYGMeshCasts.Overlaps(this, capsule);
        }

        /// <summary>
        /// Squared distance from a world-space point to the nearest triangle.
        /// Used by overlap and rigid-body narrowphase queries.
        /// </summary>
        public FP DistanceSquared(FPVector3 point)
        {
            FP best = FP.MaxValue;
            for (int t = 0; t < _triangleBounds.Length; t++)
            {
                FPAABB bounds = _triangleBounds[t];
                FPVector3 closestBounds = new FPVector3(
                    FP.Clamp(point.x, bounds.Min.x, bounds.Max.x),
                    FP.Clamp(point.y, bounds.Min.y, bounds.Max.y),
                    FP.Clamp(point.z, bounds.Min.z, bounds.Max.z));
                FP boundsDistance = FPVector3.DistanceSquared(point, closestBounds);
                if (boundsDistance >= best)
                    continue;

                GetTriangle(t, out FPVector3 a, out FPVector3 b, out FPVector3 c);
                FPVector3 closest = CYGTriangleMath.ClosestPointOnTriangle(point, a, b, c);
                FP distance = FPVector3.DistanceSquared(point, closest);
                if (distance < best)
                    best = distance;
            }
            return best;
        }

        internal FPAABB GetTriangleBounds(int triangle)
        {
            return _triangleBounds[triangle];
        }

        internal bool RaycastTriangleBounds(FPRay ray, int triangle, FP maxDistance)
        {
            return FPRaycast.RaycastAABB(ray, _triangleBounds[triangle], maxDistance, out _);
        }
    }

    /// <summary>
    /// Registry of static meshes keyed by a stable id. Level bakers register
    /// their meshes here; simulation systems resolve them by id. Clear the
    /// registry when changing scenes or starting a new content set.
    /// </summary>
    public static class CYGMeshRegistry
    {
        private static readonly Dictionary<int, CYGStaticMesh> Meshes =
            new Dictionary<int, CYGStaticMesh>();

        public static int Count
        {
            get { return Meshes.Count; }
        }

        public static bool Register(CYGStaticMesh mesh)
        {
            if (mesh == null || mesh.Id == 0)
            {
                return false;
            }
            Meshes[mesh.Id] = mesh;
            return true;
        }

        public static bool TryGet(int id, out CYGStaticMesh mesh)
        {
            return Meshes.TryGetValue(id, out mesh);
        }

        public static CYGStaticMesh Get(int id)
        {
            return TryGet(id, out CYGStaticMesh mesh) ? mesh : null;
        }

        public static bool Remove(int id)
        {
            return Meshes.Remove(id);
        }

        public static void Clear()
        {
            Meshes.Clear();
        }

        /// <summary>Stable FNV-1a id for a level/mesh key. Never returns zero.</summary>
        public static int StableId(string key)
        {
            if (string.IsNullOrEmpty(key))
            {
                return 0;
            }

            uint hash = 2166136261u;
            for (int i = 0; i < key.Length; i++)
            {
                hash ^= key[i];
                hash *= 16777619u;
            }

            int id = unchecked((int)hash);
            return id != 0 ? id : 1;
        }
    }
}
