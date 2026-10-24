//-----------------------------------------------------------------------------
// CYGAgentSnapshot.cs
// Deep snapshots for deterministic rollback of navigation agents.
// Zero UnityEngine dependencies.
//
// Snapshots copy path waypoints and every steering timer/flag. They contain
// only plain data and fixed-point values, so capturing and restoring never
// depends on object identity or hash ordering.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    /// <summary>Deep copy of one <see cref="CYGAgent"/> state.</summary>
    public sealed class CYGAgentSnapshot
    {
        public int Id;
        public FPVector3 Position;
        public FPVector3 Velocity;
        public FP Radius;
        public FP MaxSpeed;
        public FP Acceleration;
        public FPVector3 Destination;
        public FPVector3 PathDestination;
        public bool HasDestination;
        public bool HasReachedDestination;
        public bool NeedsRepath;
        public bool PathFailed;
        public int CurrentWaypointIndex;
        public int StuckTicks;
        public int StuckRepathTicks;
        public FP WaypointTolerance;
        public FP ArrivalTolerance;
        public FP SlowdownRadius;
        public FP RepathThreshold;
        public FP SnapDistance;
        public uint FilterRegionMask;
        public uint FilterAreaMask;
        public FPVector3[] Path;
    }

    /// <summary>
    /// Snapshot of a whole <see cref="CYGAgentSystem"/>. <see cref="Checksum"/>
    /// is the system checksum at capture time and is verified on restore.
    /// </summary>
    public sealed class CYGAgentSystemSnapshot
    {
        public int TickCount;
        public uint NavMeshChecksum;
        public int SeparationIterations;
        public CYGAgentSnapshot[] Agents;
        public uint Checksum;

        public int AgentCount
        {
            get { return Agents != null ? Agents.Length : 0; }
        }
    }
}
