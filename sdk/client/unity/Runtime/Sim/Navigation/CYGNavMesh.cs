//-----------------------------------------------------------------------------
// CYGNavMesh.cs
// Immutable baked triangle navmesh with deterministic point queries, snapping,
// sorted triangle adjacency, region/area filtering and content checksum.
// Zero UnityEngine dependencies, all math is Q32.32 fixed point.
//
// LIMITATIONS (MVP):
//  - Query methods use a linear scan. This is deterministic and correct, but
//    callers with very large meshes should add a uniform grid in a later phase.
//  - The mesh is static: no dynamic obstacles, tile streaming, or runtime edits.
//  - Off-mesh links are authored data; points are not validated/snapped.
//  - TryFindTriangle projects onto the XZ plane. Use IsPointOnNavMesh with a
//    surface tolerance when vertical distance matters.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Immutable navigation mesh baked by <see cref="CYGNavMeshBuilder"/>.
    /// Data is stored in ascending triangle order and every adjacency list is
    /// sorted, so traversal and checksums are independent of hash ordering.
    /// </summary>
    public sealed class CYGNavMesh
    {
        public const int FormatVersion = 1;

        private static readonly FP EdgeEpsilon = FP.FromFraction(1, 10000);

        private readonly FPVector3[] _vertices;
        private readonly int[] _indices;
        private readonly CYGNavMeshTriangle[] _triangles;
        private readonly FPVector3[] _centroids;
        private readonly int[] _neighbors;
        private readonly CYGOffMeshLink[] _links;
        private readonly FPAABB _bounds;
        private readonly uint _checksum;

        internal CYGNavMesh(
            FPVector3[] vertices,
            int[] indices,
            CYGNavMeshTriangle[] triangles,
            FPVector3[] centroids,
            int[] neighbors,
            CYGOffMeshLink[] links,
            FPAABB bounds)
        {
            _vertices = vertices;
            _indices = indices;
            _triangles = triangles;
            _centroids = centroids;
            _neighbors = neighbors;
            _links = links;
            _bounds = bounds;
            _checksum = ComputeContentChecksum();
        }

        public int VertexCount
        {
            get { return _vertices.Length; }
        }

        public int TriangleCount
        {
            get { return _triangles.Length; }
        }

        public int LinkCount
        {
            get { return _links.Length; }
        }

        public bool IsEmpty
        {
            get { return _triangles.Length == 0; }
        }

        public FPAABB Bounds
        {
            get { return _bounds; }
        }

        /// <summary>Stable CRC32 of the baked vertex, triangle, adjacency and link data.</summary>
        public uint Checksum
        {
            get { return _checksum; }
        }

        public FPVector3 GetVertex(int index)
        {
            if (index < 0 || index >= _vertices.Length)
                throw new ArgumentOutOfRangeException(nameof(index));

            return _vertices[index];
        }

        public bool TryGetTriangle(int index, out CYGNavMeshTriangle triangle)
        {
            if (index < 0 || index >= _triangles.Length)
            {
                triangle = default(CYGNavMeshTriangle);
                return false;
            }

            triangle = _triangles[index];
            return true;
        }

        /// <summary>Returns a sorted neighbour triangle or -1 for an open edge.</summary>
        public int GetNeighbor(int triangleIndex, int slot)
        {
            if (triangleIndex < 0 || triangleIndex >= _triangles.Length)
                throw new ArgumentOutOfRangeException(nameof(triangleIndex));
            if (slot < 0 || slot >= 3)
                throw new ArgumentOutOfRangeException(nameof(slot));

            return _neighbors[(triangleIndex * 3) + slot];
        }

        public bool TryGetLink(int index, out CYGOffMeshLink link)
        {
            if (index < 0 || index >= _links.Length)
            {
                link = default(CYGOffMeshLink);
                return false;
            }

            link = _links[index];
            return true;
        }

        #region Point queries

        /// <summary>
        /// Finds the lowest-index triangle whose XZ projection contains the
        /// point. The point's Y coordinate is ignored.
        /// </summary>
        public bool TryFindTriangle(FPVector3 point, out int triangleIndex)
        {
            return TryFindTriangle(point, CYGNavQueryFilter.All, out triangleIndex);
        }

        /// <summary>
        /// Finds the lowest-index triangle accepted by the filter whose XZ
        /// projection contains the point.
        /// </summary>
        public bool TryFindTriangle(FPVector3 point, CYGNavQueryFilter filter, out int triangleIndex)
        {
            for (int i = 0; i < _triangles.Length; i++)
            {
                if (filter.Allows(_triangles[i]) && ContainsXZ(i, point))
                {
                    triangleIndex = i;
                    return true;
                }
            }

            triangleIndex = -1;
            return false;
        }

        public bool TryFindTriangle(FPVector3 point, out int triangleIndex, out CYGNavMeshTriangle triangle)
        {
            if (TryFindTriangle(point, out triangleIndex))
            {
                triangle = _triangles[triangleIndex];
                return true;
            }

            triangle = default(CYGNavMeshTriangle);
            return false;
        }

        /// <summary>True when the XZ projection of the point is inside the mesh.</summary>
        public bool IsPointOnNavMesh(FPVector3 point)
        {
            int triangleIndex;
            return TryFindTriangle(point, out triangleIndex);
        }

        public bool IsPointOnNavMesh(FPVector3 point, CYGNavQueryFilter filter)
        {
            int triangleIndex;
            return TryFindTriangle(point, filter, out triangleIndex);
        }

        /// <summary>
        /// True when the point projects inside a triangle and its distance to
        /// that triangle's plane is at most <paramref name="maxSurfaceDistance"/>.
        /// </summary>
        public bool IsPointOnNavMesh(FPVector3 point, FP maxSurfaceDistance)
        {
            return IsPointOnNavMesh(point, maxSurfaceDistance, CYGNavQueryFilter.All);
        }

        public bool IsPointOnNavMesh(FPVector3 point, FP maxSurfaceDistance, CYGNavQueryFilter filter)
        {
            if (maxSurfaceDistance < FP.Zero)
                return false;

            int triangleIndex;
            if (!TryFindTriangle(point, filter, out triangleIndex))
                return false;

            return PlaneDistanceSquared(triangleIndex, point) <= maxSurfaceDistance * maxSurfaceDistance;
        }

        /// <summary>
        /// Finds the closest point on any triangle. The closest point and the
        /// triangle index are returned only when the 3D distance is within
        /// <paramref name="maxDistance"/>. Ties keep the lowest triangle index.
        /// </summary>
        public bool SnapToNavMesh(FPVector3 point, FP maxDistance, out FPVector3 snapped)
        {
            int triangleIndex;
            return SnapToNavMesh(point, maxDistance, out snapped, out triangleIndex);
        }

        public bool SnapToNavMesh(FPVector3 point, FP maxDistance, out FPVector3 snapped, out int triangleIndex)
        {
            return SnapToNavMesh(point, maxDistance, CYGNavQueryFilter.All, out snapped, out triangleIndex);
        }

        public bool SnapToNavMesh(
            FPVector3 point,
            FP maxDistance,
            CYGNavQueryFilter filter,
            out FPVector3 snapped)
        {
            int triangleIndex;
            return SnapToNavMesh(point, maxDistance, filter, out snapped, out triangleIndex);
        }

        public bool SnapToNavMesh(
            FPVector3 point,
            FP maxDistance,
            CYGNavQueryFilter filter,
            out FPVector3 snapped,
            out int triangleIndex)
        {
            snapped = point;
            triangleIndex = -1;
            if (maxDistance < FP.Zero || _triangles.Length == 0)
                return false;

            FP bestDistanceSquared = FP.MaxValue;
            FPVector3 bestPoint = point;

            for (int i = 0; i < _triangles.Length; i++)
            {
                if (!filter.Allows(_triangles[i]))
                    continue;

                FPVector3 candidate = ClosestPointOnTriangle(i, point);
                FP distanceSquared = FPVector3.DistanceSquared(candidate, point);
                if (distanceSquared < bestDistanceSquared)
                {
                    bestDistanceSquared = distanceSquared;
                    bestPoint = candidate;
                    triangleIndex = i;
                }
            }

            if (triangleIndex < 0)
                return false;

            FP maxDistanceSquared = maxDistance * maxDistance;
            if (bestDistanceSquared > maxDistanceSquared)
            {
                triangleIndex = -1;
                return false;
            }

            snapped = bestPoint;
            return true;
        }

        public FPVector3 ClosestPointOnTriangle(int triangleIndex, FPVector3 point)
        {
            if (triangleIndex < 0 || triangleIndex >= _triangles.Length)
                throw new ArgumentOutOfRangeException(nameof(triangleIndex));

            CYGNavMeshTriangle triangle = _triangles[triangleIndex];
            FPVector3 a = _vertices[triangle.Vertex0];
            FPVector3 b = _vertices[triangle.Vertex1];
            FPVector3 c = _vertices[triangle.Vertex2];

            FPVector3 ab = b - a;
            FPVector3 ac = c - a;
            FPVector3 ap = point - a;
            FP d1 = FPVector3.Dot(ab, ap);
            FP d2 = FPVector3.Dot(ac, ap);
            if (d1 <= FP.Zero && d2 <= FP.Zero)
                return a;

            FPVector3 bp = point - b;
            FP d3 = FPVector3.Dot(ab, bp);
            FP d4 = FPVector3.Dot(ac, bp);
            if (d3 >= FP.Zero && d4 <= d3)
                return b;

            FP vc = (d1 * d4) - (d3 * d2);
            if (vc <= FP.Zero && d1 >= FP.Zero && d3 <= FP.Zero)
            {
                FP denominator = d1 - d3;
                if (denominator == FP.Zero)
                    return a;
                FP v = d1 / denominator;
                return a + ab * v;
            }

            FPVector3 cp = point - c;
            FP d5 = FPVector3.Dot(ab, cp);
            FP d6 = FPVector3.Dot(ac, cp);
            if (d6 >= FP.Zero && d5 <= d6)
                return c;

            FP vb = (d5 * d2) - (d1 * d6);
            if (vb <= FP.Zero && d2 >= FP.Zero && d6 <= FP.Zero)
            {
                FP denominator = d2 - d6;
                if (denominator == FP.Zero)
                    return a;
                FP w = d2 / denominator;
                return a + ac * w;
            }

            FP va = (d3 * d6) - (d5 * d4);
            if (va <= FP.Zero && (d4 - d3) >= FP.Zero && (d5 - d6) >= FP.Zero)
            {
                FP denominator = (d4 - d3) + (d5 - d6);
                if (denominator == FP.Zero)
                    return b;
                FP w = (d4 - d3) / denominator;
                return b + (c - b) * w;
            }

            FP total = va + vb + vc;
            if (total == FP.Zero)
                return a;

            FP baryV = vb / total;
            FP baryW = vc / total;
            return a + ab * baryV + ac * baryW;
        }

        private bool ContainsXZ(int triangleIndex, FPVector3 point)
        {
            CYGNavMeshTriangle triangle = _triangles[triangleIndex];
            FPVector3 a = _vertices[triangle.Vertex0];
            FPVector3 b = _vertices[triangle.Vertex1];
            FPVector3 c = _vertices[triangle.Vertex2];

            FP c1 = Cross2D(b - a, point - a);
            FP c2 = Cross2D(c - b, point - b);
            FP c3 = Cross2D(a - c, point - c);

            bool hasNegative = c1 < -EdgeEpsilon || c2 < -EdgeEpsilon || c3 < -EdgeEpsilon;
            bool hasPositive = c1 > EdgeEpsilon || c2 > EdgeEpsilon || c3 > EdgeEpsilon;
            return !(hasNegative && hasPositive);
        }

        private FP PlaneDistanceSquared(int triangleIndex, FPVector3 point)
        {
            CYGNavMeshTriangle triangle = _triangles[triangleIndex];
            FPVector3 a = _vertices[triangle.Vertex0];
            FPVector3 b = _vertices[triangle.Vertex1];
            FPVector3 c = _vertices[triangle.Vertex2];

            FPVector3 normal = FPVector3.Cross(b - a, c - a);
            FP lengthSquared = normal.SqrMagnitude;
            if (lengthSquared <= FP.Zero)
                return FP.Zero;

            FP dot = FPVector3.Dot(point - a, normal);
            return (dot * dot) / lengthSquared;
        }

        private static FP Cross2D(FPVector3 a, FPVector3 b)
        {
            return (a.x * b.z) - (a.z * b.x);
        }

        #endregion

        #region Internal pathfinder access

        internal FPVector3 GetCentroid(int triangleIndex)
        {
            return _centroids[triangleIndex];
        }

        internal int GetNeighborUnchecked(int triangleIndex, int slot)
        {
            return _neighbors[(triangleIndex * 3) + slot];
        }

        internal CYGOffMeshLink GetLinkUnchecked(int index)
        {
            return _links[index];
        }

        /// <summary>
        /// Returns the shared edge of adjacent triangles oriented by the first
        /// triangle's winding (a -> b). For travel from triA to triB the portal
        /// left point is b and the right point is a.
        /// </summary>
        internal bool TryGetSharedEdge(int triangleA, int triangleB, out FPVector3 left, out FPVector3 right)
        {
            left = FPVector3.Zero;
            right = FPVector3.Zero;
            if (triangleA < 0 || triangleA >= _triangles.Length
                || triangleB < 0 || triangleB >= _triangles.Length
                || triangleA == triangleB)
            {
                return false;
            }

            CYGNavMeshTriangle a = _triangles[triangleA];
            CYGNavMeshTriangle b = _triangles[triangleB];

            for (int edge = 0; edge < 3; edge++)
            {
                int a0 = a.GetVertexIndex(edge);
                int a1 = a.GetVertexIndex((edge + 1) % 3);

                for (int other = 0; other < 3; other++)
                {
                    int b0 = b.GetVertexIndex(other);
                    int b1 = b.GetVertexIndex((other + 1) % 3);

                    // Adjacent winding must be opposite. Same-order matches are
                    // non-manifold duplicates and are ignored by the funnel.
                    if (a0 == b1 && a1 == b0)
                    {
                        left = _vertices[a1];
                        right = _vertices[a0];
                        return true;
                    }
                }
            }

            return false;
        }

        #endregion

        #region Checksum

        private uint ComputeContentChecksum()
        {
            uint crc = 0xFFFFFFFFu;
            CYGChecksum.HashInt(ref crc, FormatVersion);
            CYGChecksum.HashInt(ref crc, _vertices.Length);
            CYGChecksum.HashInt(ref crc, _triangles.Length);
            CYGChecksum.HashInt(ref crc, _links.Length);

            for (int i = 0; i < _vertices.Length; i++)
                _vertices[i].ComputeChecksum(ref crc);

            for (int i = 0; i < _indices.Length; i++)
                CYGChecksum.HashInt(ref crc, _indices[i]);

            for (int i = 0; i < _triangles.Length; i++)
            {
                CYGNavMeshTriangle triangle = _triangles[i];
                CYGChecksum.HashInt(ref crc, triangle.Vertex0);
                CYGChecksum.HashInt(ref crc, triangle.Vertex1);
                CYGChecksum.HashInt(ref crc, triangle.Vertex2);
                CYGChecksum.HashInt(ref crc, triangle.RegionId);
                CYGChecksum.HashUInt(ref crc, triangle.AreaMask);
                CYGChecksum.HashLong(ref crc, triangle.Cost.RawValue);
                _centroids[i].ComputeChecksum(ref crc);
            }

            for (int i = 0; i < _neighbors.Length; i++)
                CYGChecksum.HashInt(ref crc, _neighbors[i]);

            for (int i = 0; i < _links.Length; i++)
            {
                CYGOffMeshLink link = _links[i];
                CYGChecksum.HashInt(ref crc, link.FromTriangle);
                CYGChecksum.HashInt(ref crc, link.ToTriangle);
                link.FromPoint.ComputeChecksum(ref crc);
                link.ToPoint.ComputeChecksum(ref crc);
                CYGChecksum.HashLong(ref crc, link.Cost.RawValue);
                CYGChecksum.HashByte(ref crc, link.Bidirectional ? (byte)1 : (byte)0);
            }

            _bounds.ComputeChecksum(ref crc);
            return crc ^ 0xFFFFFFFFu;
        }

        #endregion
    }
}
