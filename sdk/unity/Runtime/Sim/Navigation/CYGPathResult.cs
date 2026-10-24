//-----------------------------------------------------------------------------
// CYGPathResult.cs
// Query result returned by CYGPathfinder.
// Zero UnityEngine dependencies.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    public enum CYGPathStatus
    {
        Success = 0,
        StartOutsideNavMesh = 1,
        EndOutsideNavMesh = 2,
        StartFiltered = 3,
        EndFiltered = 4,
        NoPath = 5,
        InvalidMesh = 6
    }

    /// <summary>
    /// Immutable-by-convention path query result. Waypoints exclude the start
    /// position and include the destination when it differs from the start.
    /// Arrays are owned by the result; callers may read but should not mutate
    /// them if the result is stored or hashed.
    /// </summary>
    public sealed class CYGPathResult
    {
        private static readonly FPVector3[] EmptyWaypoints = new FPVector3[0];
        private static readonly int[] EmptyTriangles = new int[0];

        public CYGPathStatus Status;
        public FPVector3[] Waypoints;
        public int WaypointCount;
        public int[] TrianglePath;
        public int TrianglePathCount;
        public FP PathCost;
        public int StartTriangle;
        public int EndTriangle;

        public bool Success
        {
            get { return Status == CYGPathStatus.Success; }
        }

        internal CYGPathResult(
            CYGPathStatus status,
            FPVector3[] waypoints,
            int waypointCount,
            int[] trianglePath,
            int trianglePathCount,
            FP pathCost,
            int startTriangle,
            int endTriangle)
        {
            Status = status;
            Waypoints = waypoints ?? EmptyWaypoints;
            WaypointCount = waypointCount;
            TrianglePath = trianglePath ?? EmptyTriangles;
            TrianglePathCount = trianglePathCount;
            PathCost = pathCost;
            StartTriangle = startTriangle;
            EndTriangle = endTriangle;
        }

        internal static CYGPathResult Failure(CYGPathStatus status)
        {
            return new CYGPathResult(status, EmptyWaypoints, 0, EmptyTriangles, 0, FP.Zero, -1, -1);
        }

        public FPVector3 GetWaypoint(int index)
        {
            if (index < 0 || index >= WaypointCount)
                throw new ArgumentOutOfRangeException(nameof(index));

            return Waypoints[index];
        }

        public int GetTriangle(int index)
        {
            if (index < 0 || index >= TrianglePathCount)
                throw new ArgumentOutOfRangeException(nameof(index));

            return TrianglePath[index];
        }
    }
}
