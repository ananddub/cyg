//-----------------------------------------------------------------------------
// CYGAgentSystem.cs
// Deterministic navigation agent system: ordered stepping, reciprocal
// separation, checksums and rollback snapshots. Zero UnityEngine dependencies.
//
// Agents are stepped in insertion order. Separation is a fixed number of
// pairwise passes in the same order, so the entire Tick is deterministic for a
// given list order, fixed-point inputs and delta time.
//
// LIMITATIONS (MVP):
//  - No dynamic obstacles, crowd avoidance, or spatial acceleration structure.
//    Separation is O(n^2) per pass, which is fine for small groups only.
//  - Separation is horizontal and can push agents off the navmesh.
//  - Repathing is immediate when requested; there is no time-sliced budget.
//-----------------------------------------------------------------------------

using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    public sealed class CYGAgentSystem
    {
        private const int FormatVersion = 1;

        private readonly CYGNavMesh _mesh;
        private readonly CYGPathfinder _pathfinder;
        private readonly List<CYGAgent> _agents = new List<CYGAgent>();
        private int _nextAgentId;
        private int _tickCount;
        private int _separationIterations = 2;

        public CYGAgentSystem(CYGNavMesh mesh)
        {
            if (mesh == null)
                throw new ArgumentNullException(nameof(mesh));

            _mesh = mesh;
            _pathfinder = new CYGPathfinder(mesh);
        }

        public CYGNavMesh NavMesh
        {
            get { return _mesh; }
        }

        public CYGPathfinder Pathfinder
        {
            get { return _pathfinder; }
        }

        public IReadOnlyList<CYGAgent> Agents
        {
            get { return _agents; }
        }

        public int AgentCount
        {
            get { return _agents.Count; }
        }

        public int TickCount
        {
            get { return _tickCount; }
        }

        /// <summary>Pairwise separation passes per tick. Zero disables separation.</summary>
        public int SeparationIterations
        {
            get { return _separationIterations; }
            set
            {
                if (value < 0)
                    throw new ArgumentOutOfRangeException(nameof(value));
                _separationIterations = value;
            }
        }

        public CYGAgent AddAgent(FPVector3 position, FP radius, FP maxSpeed, FP acceleration)
        {
            CYGAgent agent = new CYGAgent(position, radius, maxSpeed, acceleration);
            AddAgent(agent);
            return agent;
        }

        public void AddAgent(CYGAgent agent)
        {
            if (agent == null)
                throw new ArgumentNullException(nameof(agent));
            if (_agents.Contains(agent))
                throw new ArgumentException("Agent is already part of this system.", nameof(agent));

            agent.Id = _nextAgentId++;
            _agents.Add(agent);
        }

        public bool RemoveAgent(CYGAgent agent)
        {
            if (agent == null)
                return false;

            return _agents.Remove(agent);
        }

        public void ClearAgents()
        {
            _agents.Clear();
        }

        public void Tick(FP deltaTime)
        {
            if (deltaTime < FP.Zero)
                throw new ArgumentOutOfRangeException(nameof(deltaTime), "Delta time must be non-negative.");

            for (int i = 0; i < _agents.Count; i++)
                _agents[i].Step(deltaTime, _mesh, _pathfinder);

            ResolveSeparations();
            _tickCount++;
        }

        private void ResolveSeparations()
        {
            for (int pass = 0; pass < _separationIterations; pass++)
            {
                for (int i = 0; i < _agents.Count; i++)
                {
                    CYGAgent a = _agents[i];
                    for (int j = i + 1; j < _agents.Count; j++)
                    {
                        CYGAgent b = _agents[j];
                        FP dx = b.Position.x - a.Position.x;
                        FP dz = b.Position.z - a.Position.z;
                        FP minimumDistance = a.Radius + b.Radius;
                        FP distanceSquared = (dx * dx) + (dz * dz);
                        if (distanceSquared >= minimumDistance * minimumDistance)
                            continue;

                        FP distance = FP.Sqrt(distanceSquared);
                        FPVector3 direction;
                        FP overlap;

                        if (distance > FP.Zero)
                        {
                            direction = new FPVector3(dx / distance, FP.Zero, dz / distance);
                            overlap = minimumDistance - distance;
                        }
                        else
                        {
                            // Coincident agents need a deterministic escape axis.
                            direction = new FPVector3(FP.One, FP.Zero, FP.Zero);
                            overlap = minimumDistance;
                        }

                        FPVector3 push = direction * (overlap * FP.Half);
                        a.Position = a.Position - push;
                        b.Position = b.Position + push;
                    }
                }
            }
        }

        public uint ComputeChecksum()
        {
            uint crc = 0xFFFFFFFFu;
            CYGChecksum.HashInt(ref crc, FormatVersion);
            CYGChecksum.HashInt(ref crc, _tickCount);
            CYGChecksum.HashInt(ref crc, _agents.Count);
            CYGChecksum.HashInt(ref crc, _separationIterations);
            CYGChecksum.HashUInt(ref crc, _mesh.Checksum);

            for (int i = 0; i < _agents.Count; i++)
                _agents[i].ComputeChecksum(ref crc);

            return crc ^ 0xFFFFFFFFu;
        }

        public CYGAgentSystemSnapshot CaptureSnapshot()
        {
            CYGAgentSnapshot[] agents = new CYGAgentSnapshot[_agents.Count];
            for (int i = 0; i < _agents.Count; i++)
                agents[i] = _agents[i].CaptureSnapshot();

            return new CYGAgentSystemSnapshot
            {
                TickCount = _tickCount,
                NavMeshChecksum = _mesh.Checksum,
                SeparationIterations = _separationIterations,
                Agents = agents,
                Checksum = ComputeChecksum()
            };
        }

        /// <summary>
        /// Restores a snapshot. Returns false without changing the system when
        /// the snapshot belongs to a different mesh. On success the restored
        /// checksum is verified against the captured checksum.
        /// </summary>
        public bool RestoreSnapshot(CYGAgentSystemSnapshot snapshot)
        {
            if (snapshot == null || snapshot.Agents == null)
                return false;
            if (snapshot.NavMeshChecksum != _mesh.Checksum)
                return false;
            if (snapshot.SeparationIterations < 0)
                return false;

            for (int i = 0; i < snapshot.Agents.Length; i++)
            {
                if (snapshot.Agents[i] == null)
                    return false;
            }

            _agents.Clear();
            int nextId = 0;
            for (int i = 0; i < snapshot.Agents.Length; i++)
            {
                CYGAgentSnapshot data = snapshot.Agents[i];
                CYGAgent agent = new CYGAgent(
                    data.Position,
                    data.Radius,
                    data.MaxSpeed,
                    data.Acceleration);
                agent.RestoreSnapshot(data);
                _agents.Add(agent);
                if (data.Id >= nextId)
                    nextId = data.Id + 1;
            }

            _tickCount = snapshot.TickCount;
            _separationIterations = snapshot.SeparationIterations;
            _nextAgentId = nextId;

            return ComputeChecksum() == snapshot.Checksum;
        }
    }
}
