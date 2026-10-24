//-----------------------------------------------------------------------------
// CYGAgent.cs
// Deterministic navmesh-following agent. Zero UnityEngine dependencies.
//
// The agent owns its path buffer, follows waypoints with acceleration-limited
// steering, and repaths when the destination moves or when it has been stuck
// for a number of simulation ticks. Separation between agents is applied by
// CYGAgentSystem after all agents have stepped.
//
// LIMITATIONS (MVP):
//  - The agent is not physically constrained to the navmesh. Separation pushes
//    and external writes to Position can leave it outside; the next repath
//    then fails with PathFailed until it is placed back on the mesh.
//  - Off-mesh links are followed as straight lines between their exit points.
//  - No avoidance of dynamic obstacles or other agents while moving; only the
//    deterministic reciprocal separation pass prevents overlap.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    public sealed class CYGAgent
    {
        private FPVector3 _destination;
        private FPVector3 _pathDestination;
        private FPVector3[] _path = new FPVector3[0];
        private int _pathCount;
        private int _currentWaypointIndex;
        private int _stuckTicks;
        private bool _hasReachedDestination;
        private bool _needsRepath;
        private bool _pathFailed;

        public CYGAgent()
            : this(FPVector3.Zero, FP.FromFraction(1, 2), FP.FromInt(3), FP.FromInt(10))
        {
        }

        public CYGAgent(FPVector3 position, FP radius, FP maxSpeed, FP acceleration)
        {
            if (radius < FP.Zero)
                throw new ArgumentOutOfRangeException(nameof(radius), "Agent radius must be non-negative.");
            if (maxSpeed <= FP.Zero)
                throw new ArgumentOutOfRangeException(nameof(maxSpeed), "Agent max speed must be positive.");
            if (acceleration <= FP.Zero)
                throw new ArgumentOutOfRangeException(nameof(acceleration), "Agent acceleration must be positive.");

            Position = position;
            Velocity = FPVector3.Zero;
            Radius = radius;
            MaxSpeed = maxSpeed;
            Acceleration = acceleration;
            _destination = position;
            WaypointTolerance = FP.FromFraction(1, 5);
            ArrivalTolerance = FP.FromFraction(1, 4);
            SlowdownRadius = FP.FromInt(2);
            StuckRepathTicks = 30;
            RepathThreshold = FP.FromFraction(1, 2);
            Filter = CYGNavQueryFilter.All;
            SnapDistance = FP.Zero;
        }

        public int Id { get; internal set; } = -1;

        public FPVector3 Position;
        public FPVector3 Velocity;
        public FP Radius;
        public FP MaxSpeed;
        public FP Acceleration;

        /// <summary>
        /// Intended destination. Changing this value while a destination is
        /// active marks the agent for repathing.
        /// </summary>
        public FPVector3 Destination
        {
            get { return _destination; }
            set
            {
                if (HasDestination && value != _destination)
                    MarkDestinationChanged(value);
                else
                    _destination = value;
            }
        }

        public bool HasDestination;

        public FP WaypointTolerance;
        public FP ArrivalTolerance;
        public FP SlowdownRadius;
        public int StuckRepathTicks;
        public FP RepathThreshold;
        public CYGNavQueryFilter Filter;
        public FP SnapDistance;

        public bool HasReachedDestination
        {
            get { return _hasReachedDestination; }
        }

        public bool HasPath
        {
            get { return _pathCount > 0 && _currentWaypointIndex < _pathCount; }
        }

        public bool PathFailed
        {
            get { return _pathFailed; }
        }

        public int CurrentWaypointIndex
        {
            get { return _currentWaypointIndex; }
        }

        public int PathWaypointCount
        {
            get { return _pathCount; }
        }

        public FPVector3 GetPathWaypoint(int index)
        {
            if (index < 0 || index >= _pathCount)
                throw new ArgumentOutOfRangeException(nameof(index));

            return _path[index];
        }

        public void SetDestination(FPVector3 destination)
        {
            if (HasDestination && destination == _destination)
                return;

            MarkDestinationChanged(destination);
            HasDestination = true;
        }

        private void MarkDestinationChanged(FPVector3 destination)
        {
            _destination = destination;
            _pathDestination = destination;
            _pathCount = 0;
            _currentWaypointIndex = 0;
            _stuckTicks = 0;
            _hasReachedDestination = false;
            _needsRepath = true;
            _pathFailed = false;
        }

        public void ClearDestination()
        {
            HasDestination = false;
            _destination = Position;
            _pathDestination = Position;
            _pathCount = 0;
            _currentWaypointIndex = 0;
            _stuckTicks = 0;
            _hasReachedDestination = false;
            _needsRepath = false;
            _pathFailed = false;
        }

        /// <summary>
        /// Discards cached path and stuck state so the next step rebuilds the
        /// path from the current position. Used by the ECS bridge, where the
        /// authoritative position is restored from a snapshot.
        /// </summary>
        public void ForceRepath()
        {
            _needsRepath = true;
            _pathCount = 0;
            _currentWaypointIndex = 0;
            _stuckTicks = 0;
            _pathFailed = false;
        }

        internal void Step(FP deltaTime, CYGNavMesh mesh, CYGPathfinder pathfinder)
        {
            if (deltaTime < FP.Zero)
                throw new ArgumentOutOfRangeException(nameof(deltaTime), "Delta time must be non-negative.");

            if (!HasDestination)
            {
                Decelerate(deltaTime);
                return;
            }

            FP destinationDistance = FPVector3.Distance(Position, _destination);
            if (destinationDistance <= ArrivalTolerance)
            {
                _hasReachedDestination = true;
                _currentWaypointIndex = _pathCount;
                _stuckTicks = 0;
                Velocity = FPVector3.Zero;
                return;
            }

            _hasReachedDestination = false;

            if (_needsRepath)
                RebuildPath(mesh, pathfinder);

            if (_pathFailed)
            {
                Decelerate(deltaTime);
                IncrementStuck(destinationDistance);
                if (_stuckTicks >= StuckRepathTicks && StuckRepathTicks > 0)
                {
                    _needsRepath = true;
                    _stuckTicks = 0;
                }

                return;
            }

            while (_currentWaypointIndex < _pathCount
                && FPVector3.Distance(Position, _path[_currentWaypointIndex]) <= WaypointTolerance)
            {
                _currentWaypointIndex++;
            }

            if (_currentWaypointIndex >= _pathCount)
            {
                // The path was exhausted without entering the arrival radius.
                // This can happen when a waypoint is skipped at high speed.
                IncrementStuck(destinationDistance);
                if (_stuckTicks >= StuckRepathTicks && StuckRepathTicks > 0)
                {
                    _needsRepath = true;
                    _stuckTicks = 0;
                }

                Decelerate(deltaTime);
                return;
            }

            FPVector3 waypoint = _path[_currentWaypointIndex];
            FPVector3 toWaypoint = waypoint - Position;
            FP waypointDistance = toWaypoint.Magnitude;

            FP desiredSpeed = MaxSpeed;
            if (waypointDistance < SlowdownRadius && SlowdownRadius > FP.Zero)
                desiredSpeed = MaxSpeed * (waypointDistance / SlowdownRadius);
            if (destinationDistance < SlowdownRadius && SlowdownRadius > FP.Zero)
            {
                FP destinationSpeed = MaxSpeed * (destinationDistance / SlowdownRadius);
                if (destinationSpeed < desiredSpeed)
                    desiredSpeed = destinationSpeed;
            }

            FPVector3 direction = toWaypoint.Normalized;
            FPVector3 desiredVelocity = direction * desiredSpeed;
            FP maxVelocityDelta = Acceleration * deltaTime;
            Velocity = FPVector3.MoveTowards(Velocity, desiredVelocity, maxVelocityDelta);
            Position = Position + (Velocity * deltaTime);

            if (FPVector3.Distance(Position, _destination) <= ArrivalTolerance)
            {
                _hasReachedDestination = true;
                _currentWaypointIndex = _pathCount;
                _stuckTicks = 0;
                Velocity = FPVector3.Zero;
                return;
            }

            IncrementStuck(destinationDistance);
            if (_stuckTicks >= StuckRepathTicks && StuckRepathTicks > 0)
            {
                _needsRepath = true;
                _stuckTicks = 0;
            }
        }

        private void RebuildPath(CYGNavMesh mesh, CYGPathfinder pathfinder)
        {
            _needsRepath = false;
            CYGPathResult result = pathfinder.FindPath(Position, _destination, Filter, SnapDistance);
            if (!result.Success)
            {
                _pathCount = 0;
                _currentWaypointIndex = 0;
                _pathFailed = true;
                return;
            }

            int count = result.WaypointCount;
            if (_path.Length < count)
                _path = new FPVector3[Math.Max(count, 4)];
            for (int i = 0; i < count; i++)
                _path[i] = result.Waypoints[i];

            _pathCount = count;
            _currentWaypointIndex = 0;
            _pathDestination = _destination;
            _pathFailed = false;
        }

        private void IncrementStuck(FP destinationDistance)
        {
            // Counting from the distance measured before this tick's movement
            // keeps the counter independent of delta-time rounding.
            FP slowThreshold = MaxSpeed * FP.FromFraction(1, 8);
            bool slow = Velocity.Magnitude < slowThreshold;
            bool far = destinationDistance > SlowdownRadius;
            if (slow && far)
                _stuckTicks++;
            else
                _stuckTicks = 0;
        }

        private void Decelerate(FP deltaTime)
        {
            Velocity = FPVector3.MoveTowards(Velocity, FPVector3.Zero, Acceleration * deltaTime);
            Position = Position + (Velocity * deltaTime);
        }

        #region Checksum and snapshots

        internal void ComputeChecksum(ref uint crc)
        {
            CYGChecksum.HashInt(ref crc, Id);
            Position.ComputeChecksum(ref crc);
            Velocity.ComputeChecksum(ref crc);
            Radius.ComputeChecksum(ref crc);
            MaxSpeed.ComputeChecksum(ref crc);
            Acceleration.ComputeChecksum(ref crc);
            _destination.ComputeChecksum(ref crc);
            _pathDestination.ComputeChecksum(ref crc);
            CYGChecksum.HashInt(ref crc, HasDestination ? 1 : 0);
            CYGChecksum.HashInt(ref crc, _hasReachedDestination ? 1 : 0);
            CYGChecksum.HashInt(ref crc, _needsRepath ? 1 : 0);
            CYGChecksum.HashInt(ref crc, _pathFailed ? 1 : 0);
            CYGChecksum.HashInt(ref crc, _currentWaypointIndex);
            CYGChecksum.HashInt(ref crc, _stuckTicks);
            CYGChecksum.HashInt(ref crc, _pathCount);
            CYGChecksum.HashInt(ref crc, StuckRepathTicks);
            WaypointTolerance.ComputeChecksum(ref crc);
            ArrivalTolerance.ComputeChecksum(ref crc);
            SlowdownRadius.ComputeChecksum(ref crc);
            RepathThreshold.ComputeChecksum(ref crc);
            SnapDistance.ComputeChecksum(ref crc);
            CYGChecksum.HashUInt(ref crc, Filter.RegionMask);
            CYGChecksum.HashUInt(ref crc, Filter.AreaMask);

            for (int i = 0; i < _pathCount; i++)
                _path[i].ComputeChecksum(ref crc);
        }

        internal CYGAgentSnapshot CaptureSnapshot()
        {
            FPVector3[] path = new FPVector3[_pathCount];
            for (int i = 0; i < _pathCount; i++)
                path[i] = _path[i];

            return new CYGAgentSnapshot
            {
                Id = Id,
                Position = Position,
                Velocity = Velocity,
                Radius = Radius,
                MaxSpeed = MaxSpeed,
                Acceleration = Acceleration,
                Destination = _destination,
                PathDestination = _pathDestination,
                HasDestination = HasDestination,
                HasReachedDestination = _hasReachedDestination,
                NeedsRepath = _needsRepath,
                PathFailed = _pathFailed,
                CurrentWaypointIndex = _currentWaypointIndex,
                StuckTicks = _stuckTicks,
                StuckRepathTicks = StuckRepathTicks,
                WaypointTolerance = WaypointTolerance,
                ArrivalTolerance = ArrivalTolerance,
                SlowdownRadius = SlowdownRadius,
                RepathThreshold = RepathThreshold,
                SnapDistance = SnapDistance,
                FilterRegionMask = Filter.RegionMask,
                FilterAreaMask = Filter.AreaMask,
                Path = path
            };
        }

        internal void RestoreSnapshot(CYGAgentSnapshot snapshot)
        {
            if (snapshot == null)
                throw new ArgumentNullException(nameof(snapshot));

            Id = snapshot.Id;
            Position = snapshot.Position;
            Velocity = snapshot.Velocity;
            Radius = snapshot.Radius;
            MaxSpeed = snapshot.MaxSpeed;
            Acceleration = snapshot.Acceleration;
            _destination = snapshot.Destination;
            _pathDestination = snapshot.PathDestination;
            HasDestination = snapshot.HasDestination;
            _hasReachedDestination = snapshot.HasReachedDestination;
            _needsRepath = snapshot.NeedsRepath;
            _pathFailed = snapshot.PathFailed;
            _currentWaypointIndex = snapshot.CurrentWaypointIndex;
            _stuckTicks = snapshot.StuckTicks;
            StuckRepathTicks = snapshot.StuckRepathTicks;
            WaypointTolerance = snapshot.WaypointTolerance;
            ArrivalTolerance = snapshot.ArrivalTolerance;
            SlowdownRadius = snapshot.SlowdownRadius;
            RepathThreshold = snapshot.RepathThreshold;
            SnapDistance = snapshot.SnapDistance;
            Filter = new CYGNavQueryFilter
            {
                RegionMask = snapshot.FilterRegionMask,
                AreaMask = snapshot.FilterAreaMask
            };

            int count = snapshot.Path != null ? snapshot.Path.Length : 0;
            if (_path.Length < count)
                _path = new FPVector3[Math.Max(count, 4)];
            for (int i = 0; i < count; i++)
                _path[i] = snapshot.Path[i];
            _pathCount = count;
        }

        #endregion
    }
}
