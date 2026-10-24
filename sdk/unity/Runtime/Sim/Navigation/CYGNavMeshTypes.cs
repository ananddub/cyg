//-----------------------------------------------------------------------------
// CYGNavMeshTypes.cs
// Value types shared by the deterministic navigation runtime.
// Zero UnityEngine dependencies, all simulation math is Q32.32 fixed point.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// One baked navigation triangle. Vertex indices refer to
    /// <see cref="CYGNavMesh"/> vertices. RegionId must be in [0, 31] because
    /// query filters use a 32-bit region mask. AreaMask is a free-form set of
    /// gameplay area bits (for example walk / water / road).
    /// </summary>
    public struct CYGNavMeshTriangle
    {
        public int Vertex0;
        public int Vertex1;
        public int Vertex2;
        public int RegionId;
        public uint AreaMask;
        public FP Cost;

        public CYGNavMeshTriangle(int vertex0, int vertex1, int vertex2, int regionId, uint areaMask, FP cost)
        {
            Vertex0 = vertex0;
            Vertex1 = vertex1;
            Vertex2 = vertex2;
            RegionId = regionId;
            AreaMask = areaMask;
            Cost = cost;
        }

        public int GetVertexIndex(int corner)
        {
            switch (corner)
            {
                case 0: return Vertex0;
                case 1: return Vertex1;
                case 2: return Vertex2;
                default: throw new ArgumentOutOfRangeException(nameof(corner));
            }
        }
    }

    /// <summary>
    /// Per-query navigation filter. A triangle is usable only when its region
    /// bit is present in RegionMask and its area mask intersects AreaMask.
    /// Use <see cref="All"/> for an unfiltered query. Both masks are exact
    /// 32-bit data in the checksum, so filtering never depends on hash order.
    /// </summary>
    public struct CYGNavQueryFilter
    {
        public uint RegionMask;
        public uint AreaMask;

        public static CYGNavQueryFilter All
        {
            get { return new CYGNavQueryFilter { RegionMask = 0xFFFFFFFFu, AreaMask = 0xFFFFFFFFu }; }
        }

        public static CYGNavQueryFilter ForRegion(int regionId)
        {
            if (regionId < 0 || regionId >= 32)
                throw new ArgumentOutOfRangeException(nameof(regionId), "Region id must be in [0, 31].");

            return new CYGNavQueryFilter
            {
                RegionMask = 1u << regionId,
                AreaMask = 0xFFFFFFFFu
            };
        }

        /// <summary>Returns a copy with the given region bit removed (blocked).</summary>
        public CYGNavQueryFilter WithoutRegion(int regionId)
        {
            if (regionId < 0 || regionId >= 32)
                throw new ArgumentOutOfRangeException(nameof(regionId), "Region id must be in [0, 31].");

            CYGNavQueryFilter copy = this;
            copy.RegionMask &= ~(1u << regionId);
            return copy;
        }

        /// <summary>Returns a copy that accepts only triangles with one of the given area bits.</summary>
        public CYGNavQueryFilter WithAreaMask(uint areaMask)
        {
            CYGNavQueryFilter copy = this;
            copy.AreaMask = areaMask;
            return copy;
        }

        public bool Allows(int regionId, uint areaMask)
        {
            if (regionId < 0 || regionId >= 32)
                return false;
            return (RegionMask & (1u << regionId)) != 0u
                && (AreaMask & areaMask) != 0u;
        }

        public bool Allows(CYGNavMeshTriangle triangle)
        {
            return Allows(triangle.RegionId, triangle.AreaMask);
        }
    }

    /// <summary>
    /// Directed off-mesh portal. The link is usable FromTriangle -> ToTriangle
    /// at the given cost. When <see cref="Bidirectional"/> is true it may also
    /// be traversed ToTriangle -> FromTriangle using ToPoint -> FromPoint.
    /// Points are expected to lie on (or at the edge of) the referenced
    /// triangles; the MVP does not validate or snap them.
    /// </summary>
    public struct CYGOffMeshLink
    {
        public int FromTriangle;
        public int ToTriangle;
        public FPVector3 FromPoint;
        public FPVector3 ToPoint;
        public FP Cost;
        public bool Bidirectional;

        public CYGOffMeshLink(
            int fromTriangle,
            int toTriangle,
            FPVector3 fromPoint,
            FPVector3 toPoint,
            FP cost,
            bool bidirectional)
        {
            FromTriangle = fromTriangle;
            ToTriangle = toTriangle;
            FromPoint = fromPoint;
            ToPoint = toPoint;
            Cost = cost;
            Bidirectional = bidirectional;
        }
    }
}
