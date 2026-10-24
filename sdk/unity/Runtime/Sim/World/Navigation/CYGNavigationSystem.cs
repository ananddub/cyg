using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// ECS-owned navigation agent state. Position, velocity and destination
    /// are authoritative; the path cache lives in the agent runtime and is
    /// rebuilt from the restored position every tick.
    /// </summary>
    public struct CYGNavAgent : ICYGComponent
    {
        public const int TypeId = -1020;
        public const uint SchemaHashValue = 0x5D38A4F1u;
        public const int SizeInBytes = 132;

        public const int HasDestinationFlag = 1;
        public const int ReachedFlag = 2;
        public const int PathFailedFlag = 4;

        public int AgentId;
        public int PlayerId;
        public int Flags;
        public FPVector3 Position;
        public FPVector3 Velocity;
        public FPVector3 Destination;
        public FP Radius;
        public FP MaxSpeed;
        public FP Acceleration;
        public FP SlowdownRadius;
        public FP WaypointTolerance;
        public FP ArrivalTolerance;

        public int ComponentTypeId
        {
            get { return TypeId; }
        }

        public uint ComponentSchemaHash
        {
            get { return SchemaHashValue; }
        }

        public int WireSize
        {
            get { return SizeInBytes; }
        }

        public bool HasDestination
        {
            get { return (Flags & HasDestinationFlag) != 0; }
            set { SetFlag(HasDestinationFlag, value); }
        }

        public bool HasReachedDestination
        {
            get { return (Flags & ReachedFlag) != 0; }
            set { SetFlag(ReachedFlag, value); }
        }

        public bool PathFailed
        {
            get { return (Flags & PathFailedFlag) != 0; }
            set { SetFlag(PathFailedFlag, value); }
        }

        private void SetFlag(int flag, bool value)
        {
            if (value)
            {
                Flags |= flag;
            }
            else
            {
                Flags &= ~flag;
            }
        }

        public int WriteTo(byte[] buffer, int offset)
        {
            CYGWire.RequireWritable(buffer, offset, SizeInBytes);
            CYGWire.WriteInt32(buffer, offset, AgentId);
            CYGWire.WriteInt32(buffer, offset + 4, PlayerId);
            CYGWire.WriteInt32(buffer, offset + 8, Flags);
            Position.WriteTo(buffer, offset + 12);
            Velocity.WriteTo(buffer, offset + 36);
            Destination.WriteTo(buffer, offset + 60);
            Radius.WriteTo(buffer, offset + 84);
            MaxSpeed.WriteTo(buffer, offset + 92);
            Acceleration.WriteTo(buffer, offset + 100);
            SlowdownRadius.WriteTo(buffer, offset + 108);
            WaypointTolerance.WriteTo(buffer, offset + 116);
            ArrivalTolerance.WriteTo(buffer, offset + 124);
            return SizeInBytes;
        }

        public void ReadFrom(byte[] buffer, int offset)
        {
            CYGWire.RequireReadable(buffer, offset, SizeInBytes);
            AgentId = CYGWire.ReadInt32(buffer, offset);
            PlayerId = CYGWire.ReadInt32(buffer, offset + 4);
            Flags = CYGWire.ReadInt32(buffer, offset + 8);
            Position.ReadFrom(buffer, offset + 12);
            Velocity.ReadFrom(buffer, offset + 36);
            Destination.ReadFrom(buffer, offset + 60);
            Radius.ReadFrom(buffer, offset + 84);
            MaxSpeed.ReadFrom(buffer, offset + 92);
            Acceleration.ReadFrom(buffer, offset + 100);
            SlowdownRadius.ReadFrom(buffer, offset + 108);
            WaypointTolerance.ReadFrom(buffer, offset + 116);
            ArrivalTolerance.ReadFrom(buffer, offset + 124);
        }

        public void ComputeChecksum(ref uint crc)
        {
            CYGChecksum.HashInt(ref crc, AgentId);
            CYGChecksum.HashInt(ref crc, PlayerId);
            CYGChecksum.HashInt(ref crc, Flags);
            Position.ComputeChecksum(ref crc);
            Velocity.ComputeChecksum(ref crc);
            Destination.ComputeChecksum(ref crc);
            Radius.ComputeChecksum(ref crc);
            MaxSpeed.ComputeChecksum(ref crc);
            Acceleration.ComputeChecksum(ref crc);
            SlowdownRadius.ComputeChecksum(ref crc);
            WaypointTolerance.ComputeChecksum(ref crc);
            ArrivalTolerance.ComputeChecksum(ref crc);
        }

        public static CYGNavAgent Create(int playerId, FPVector3 position)
        {
            return new CYGNavAgent
            {
                AgentId = -1,
                PlayerId = playerId,
                Flags = 0,
                Position = position,
                Velocity = FPVector3.Zero,
                Destination = position,
                Radius = FP.FromFraction(1, 2),
                MaxSpeed = FP.FromInt(3),
                Acceleration = FP.FromInt(10),
                SlowdownRadius = FP.FromInt(2),
                WaypointTolerance = FP.FromFraction(1, 5),
                ArrivalTolerance = FP.FromFraction(1, 4),
            };
        }
    }

    /// <summary>
    /// Steps a <see cref="CYGAgentSystem"/> from <see cref="CYGNavAgent"/>
    /// components. Positions and velocities stay in ECS, so a rollback
    /// restores navigation state; paths are recomputed from the restored
    /// position each tick instead of being cached across snapshots.
    ///
    /// The navmesh is a static asset and is not part of the world snapshot;
    /// every client must load an identical bake.
    /// </summary>
    public sealed class CYGNavigationSystem : CYGSystem
    {
        private readonly CYGNavMesh _mesh;
        private readonly CYGAgentSystem _agents;
        private readonly Dictionary<CYGEntityRef, CYGAgent> _agentByEntity =
            new Dictionary<CYGEntityRef, CYGAgent>();
        private readonly List<CYGEntityRef> _staleEntities = new List<CYGEntityRef>();
        private bool _driveFromInput = true;

        public CYGNavigationSystem(CYGNavMesh mesh, bool driveFromInput = true)
        {
            _mesh = mesh ?? throw new ArgumentNullException(nameof(mesh));
            _agents = new CYGAgentSystem(mesh);
            _driveFromInput = driveFromInput;
        }

        public CYGNavMesh NavMesh
        {
            get { return _mesh; }
        }

        public CYGAgentSystem Agents
        {
            get { return _agents; }
        }

        public bool DriveFromInput
        {
            get { return _driveFromInput; }
            set { _driveFromInput = value; }
        }

        public FP InputDestinationDistance = FP.FromInt(4);

        public override void OnInit(CYGWorld world)
        {
            if (!world.TryGetStore(CYGNavAgent.TypeId, out _))
            {
                world.Register<CYGNavAgent>();
            }
        }

        public override void OnUpdate(in CYGSystemContext context)
        {
            SyncAgentsFromEcs(context);
            _agents.Tick(context.DeltaTime);
            SyncAgentsToEcs(context.World);
            RemoveStaleAgents(context.World);
        }

        private void SyncAgentsFromEcs(in CYGSystemContext context)
        {
            CYGWorld world = context.World;
            for (int i = 0; i < world.SlotCount; i++)
            {
                CYGEntityRef entity;
                if (!world.TryGetEntityAt(i, out entity))
                {
                    continue;
                }

                CYGNavAgent component;
                if (!world.TryGet<CYGNavAgent>(entity, out component))
                {
                    continue;
                }

                CYGAgent agent = ResolveAgent(world, entity, ref component);
                agent.Position = component.Position;
                agent.Velocity = component.Velocity;
                agent.Radius = component.Radius;
                agent.MaxSpeed = component.MaxSpeed;
                agent.Acceleration = component.Acceleration;
                agent.SlowdownRadius = component.SlowdownRadius;
                agent.WaypointTolerance = component.WaypointTolerance;
                agent.ArrivalTolerance = component.ArrivalTolerance;

                FPVector3 destination = component.Destination;
                bool hasDestination = component.HasDestination;
                if (_driveFromInput && component.PlayerId > 0)
                {
                    CYGPlayerInput input;
                    if (context.TryGetInput(component.PlayerId, out input))
                    {
                        FP inputX = FP.FromFraction(input.MoveX, 1000);
                        FP inputZ = FP.FromFraction(input.MoveY, 1000);
                        destination = component.Position
                            + new FPVector3(inputX, FP.Zero, inputZ) * InputDestinationDistance;
                        hasDestination = true;
                    }
                }

                if (hasDestination)
                {
                    agent.SetDestination(destination);
                    agent.ForceRepath();
                }
                else
                {
                    agent.ClearDestination();
                }
            }
        }

        private CYGAgent ResolveAgent(CYGWorld world, CYGEntityRef entity, ref CYGNavAgent component)
        {
            CYGAgent agent;
            if (_agentByEntity.TryGetValue(entity, out agent))
            {
                if (component.AgentId != agent.Id)
                {
                    component.AgentId = agent.Id;
                    world.Set(entity, component);
                }
                return agent;
            }

            if (component.AgentId >= 0 && component.AgentId < _agents.AgentCount)
            {
                CYGAgent candidate = _agents.Agents[component.AgentId];
                if (candidate.Id == component.AgentId)
                {
                    agent = candidate;
                    _agentByEntity[entity] = agent;
                    return agent;
                }
            }

            agent = new CYGAgent();
            _agents.AddAgent(agent);
            component.AgentId = agent.Id;
            _agentByEntity[entity] = agent;
            world.Set(entity, component);
            return agent;
        }

        private void SyncAgentsToEcs(CYGWorld world)
        {
            for (int i = 0; i < world.SlotCount; i++)
            {
                CYGEntityRef entity;
                if (!world.TryGetEntityAt(i, out entity))
                {
                    continue;
                }

                CYGNavAgent component;
                if (!world.TryGet<CYGNavAgent>(entity, out component))
                {
                    continue;
                }

                CYGAgent agent;
                if (!_agentByEntity.TryGetValue(entity, out agent))
                {
                    continue;
                }

                component.Position = agent.Position;
                component.Velocity = agent.Velocity;
                component.HasDestination = agent.HasDestination;
                component.HasReachedDestination = agent.HasReachedDestination;
                component.PathFailed = agent.PathFailed;
                if (agent.HasDestination)
                {
                    component.Destination = agent.Destination;
                }
                world.Set(entity, component);
            }
        }

        private void RemoveStaleAgents(CYGWorld world)
        {
            _staleEntities.Clear();
            foreach (KeyValuePair<CYGEntityRef, CYGAgent> pair in _agentByEntity)
            {
                CYGNavAgent component;
                if (!world.TryGet<CYGNavAgent>(pair.Key, out component))
                {
                    _staleEntities.Add(pair.Key);
                }
            }

            for (int i = 0; i < _staleEntities.Count; i++)
            {
                CYGAgent agent;
                if (_agentByEntity.TryGetValue(_staleEntities[i], out agent))
                {
                    _agents.RemoveAgent(agent);
                    _agentByEntity.Remove(_staleEntities[i]);
                }
            }
        }
    }
}
