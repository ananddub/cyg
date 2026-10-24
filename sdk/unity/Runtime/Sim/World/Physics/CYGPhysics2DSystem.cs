using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// ECS-owned 2D rigid body state. The component is the authoritative
    /// record; the 2D world is updated from it before each step and the result
    /// is written back, so snapshots and rollback cover 2D physics too.
    /// </summary>
    public struct CYGPhysicsBody2D : ICYGComponent
    {
        public const int TypeId = -1011;
        public const uint SchemaHashValue = 0x4A91E2C7u;
        public const int SizeInBytes = 144;

        public const byte ActiveFlag = 1;
        public const byte AwakeFlag = 2;
        public const byte TriggerFlag = 4;

        public int BodyId;
        public int PlayerId;
        public byte BodyType;
        public byte Shape;
        public byte Flags;
        public byte Reserved;
        public FPVector2 Position;
        public FP Angle;
        public FPVector2 Velocity;
        public FP AngularVelocity;
        public FP Radius;
        public FPVector2 HalfExtents;
        public FP Mass;
        public FP Friction;
        public FP Restitution;
        public FP LinearDamping;
        public FP AngularDamping;
        public FP GravityScale;
        public int Layer;
        public int CollisionMask;
        public int SleepFrames;

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

        public bool IsActive
        {
            get { return (Flags & ActiveFlag) != 0; }
            set { SetFlag(ActiveFlag, value); }
        }

        public bool IsAwake
        {
            get { return (Flags & AwakeFlag) != 0; }
            set { SetFlag(AwakeFlag, value); }
        }

        public bool IsTrigger
        {
            get { return (Flags & TriggerFlag) != 0; }
            set { SetFlag(TriggerFlag, value); }
        }

        private void SetFlag(byte flag, bool value)
        {
            if (value)
            {
                Flags |= flag;
            }
            else
            {
                Flags = (byte)(Flags & ~flag);
            }
        }

        public int WriteTo(byte[] buffer, int offset)
        {
            CYGWire.RequireWritable(buffer, offset, SizeInBytes);
            CYGWire.WriteInt32(buffer, offset, BodyId);
            CYGWire.WriteInt32(buffer, offset + 4, PlayerId);
            CYGWire.WriteByte(buffer, offset + 8, BodyType);
            CYGWire.WriteByte(buffer, offset + 9, Shape);
            CYGWire.WriteByte(buffer, offset + 10, Flags);
            CYGWire.WriteByte(buffer, offset + 11, Reserved);
            Position.WriteTo(buffer, offset + 12);
            Angle.WriteTo(buffer, offset + 28);
            Velocity.WriteTo(buffer, offset + 36);
            AngularVelocity.WriteTo(buffer, offset + 52);
            Radius.WriteTo(buffer, offset + 60);
            HalfExtents.WriteTo(buffer, offset + 68);
            Mass.WriteTo(buffer, offset + 84);
            Friction.WriteTo(buffer, offset + 92);
            Restitution.WriteTo(buffer, offset + 100);
            LinearDamping.WriteTo(buffer, offset + 108);
            AngularDamping.WriteTo(buffer, offset + 116);
            GravityScale.WriteTo(buffer, offset + 124);
            CYGWire.WriteInt32(buffer, offset + 132, Layer);
            CYGWire.WriteInt32(buffer, offset + 136, CollisionMask);
            CYGWire.WriteInt32(buffer, offset + 140, SleepFrames);
            return SizeInBytes;
        }

        public void ReadFrom(byte[] buffer, int offset)
        {
            CYGWire.RequireReadable(buffer, offset, SizeInBytes);
            BodyId = CYGWire.ReadInt32(buffer, offset);
            PlayerId = CYGWire.ReadInt32(buffer, offset + 4);
            BodyType = CYGWire.ReadByte(buffer, offset + 8);
            Shape = CYGWire.ReadByte(buffer, offset + 9);
            Flags = CYGWire.ReadByte(buffer, offset + 10);
            Reserved = CYGWire.ReadByte(buffer, offset + 11);
            Position.ReadFrom(buffer, offset + 12);
            Angle.ReadFrom(buffer, offset + 28);
            Velocity.ReadFrom(buffer, offset + 36);
            AngularVelocity.ReadFrom(buffer, offset + 52);
            Radius.ReadFrom(buffer, offset + 60);
            HalfExtents.ReadFrom(buffer, offset + 68);
            Mass.ReadFrom(buffer, offset + 84);
            Friction.ReadFrom(buffer, offset + 92);
            Restitution.ReadFrom(buffer, offset + 100);
            LinearDamping.ReadFrom(buffer, offset + 108);
            AngularDamping.ReadFrom(buffer, offset + 116);
            GravityScale.ReadFrom(buffer, offset + 124);
            Layer = CYGWire.ReadInt32(buffer, offset + 132);
            CollisionMask = CYGWire.ReadInt32(buffer, offset + 136);
            SleepFrames = CYGWire.ReadInt32(buffer, offset + 140);
        }

        public void ComputeChecksum(ref uint crc)
        {
            CYGChecksum.HashInt(ref crc, BodyId);
            CYGChecksum.HashInt(ref crc, PlayerId);
            CYGChecksum.HashByte(ref crc, BodyType);
            CYGChecksum.HashByte(ref crc, Shape);
            CYGChecksum.HashByte(ref crc, Flags);
            CYGChecksum.HashByte(ref crc, Reserved);
            Position.ComputeChecksum(ref crc);
            Angle.ComputeChecksum(ref crc);
            Velocity.ComputeChecksum(ref crc);
            AngularVelocity.ComputeChecksum(ref crc);
            Radius.ComputeChecksum(ref crc);
            HalfExtents.ComputeChecksum(ref crc);
            Mass.ComputeChecksum(ref crc);
            Friction.ComputeChecksum(ref crc);
            Restitution.ComputeChecksum(ref crc);
            LinearDamping.ComputeChecksum(ref crc);
            AngularDamping.ComputeChecksum(ref crc);
            GravityScale.ComputeChecksum(ref crc);
            CYGChecksum.HashInt(ref crc, Layer);
            CYGChecksum.HashInt(ref crc, CollisionMask);
            CYGChecksum.HashInt(ref crc, SleepFrames);
        }

        public static CYGPhysicsBody2D DynamicCircle(int playerId, FPVector2 position, FP radius, FP mass)
        {
            return Create(playerId, CYGPhysics2DBodyType.Dynamic, CYGPhysics2DShapeType.Circle, position, FPVector2.Zero, radius, mass);
        }

        public static CYGPhysicsBody2D DynamicBox(int playerId, FPVector2 position, FPVector2 halfExtents, FP mass)
        {
            return Create(playerId, CYGPhysics2DBodyType.Dynamic, CYGPhysics2DShapeType.Box, position, halfExtents, FP.Zero, mass);
        }

        public static CYGPhysicsBody2D StaticBox(FPVector2 position, FPVector2 halfExtents)
        {
            return Create(0, CYGPhysics2DBodyType.Static, CYGPhysics2DShapeType.Box, position, halfExtents, FP.Zero, FP.Zero);
        }

        public static CYGPhysicsBody2D StaticCircle(FPVector2 position, FP radius)
        {
            return Create(0, CYGPhysics2DBodyType.Static, CYGPhysics2DShapeType.Circle, position, FPVector2.Zero, radius, FP.Zero);
        }

        private static CYGPhysicsBody2D Create(
            int playerId,
            CYGPhysics2DBodyType bodyType,
            CYGPhysics2DShapeType shape,
            FPVector2 position,
            FPVector2 halfExtents,
            FP radius,
            FP mass)
        {
            return new CYGPhysicsBody2D
            {
                BodyId = -1,
                PlayerId = playerId,
                BodyType = (byte)bodyType,
                Shape = (byte)shape,
                Flags = ActiveFlag | AwakeFlag,
                Position = position,
                Angle = FP.Zero,
                Velocity = FPVector2.Zero,
                AngularVelocity = FP.Zero,
                Radius = radius,
                HalfExtents = halfExtents,
                Mass = mass,
                Friction = FP.FromFraction(1, 2),
                Restitution = FP.Zero,
                LinearDamping = FP.Zero,
                AngularDamping = FP.Zero,
                GravityScale = FP.One,
                Layer = 0,
                CollisionMask = -1,
                SleepFrames = 0,
            };
        }
    }

    /// <summary>
    /// Steps a <see cref="CYGPhysics2DWorld"/> from <see cref="CYGPhysicsBody2D"/>
    /// components. Body state and sleep counters live in the component, so a
    /// rollback restores 2D physics exactly.
    /// </summary>
    public sealed class CYGPhysics2DSystem : CYGSystem
    {
        private readonly CYGPhysics2DWorld _physics;
        private readonly Dictionary<CYGEntityRef, int> _bodyByEntity = new Dictionary<CYGEntityRef, int>();
        private readonly List<CYGEntityRef> _staleEntities = new List<CYGEntityRef>();
        private bool _driveFromInput = true;

        public CYGPhysics2DSystem(FPVector2? gravity = null)
        {
            _physics = gravity.HasValue
                ? new CYGPhysics2DWorld(gravity.Value)
                : new CYGPhysics2DWorld();
        }

        public CYGPhysics2DWorld Physics
        {
            get { return _physics; }
        }

        public FP MoveSpeed = FP.FromInt(6);
        public FP JumpImpulse = FP.FromInt(8);

        public bool DriveFromInput
        {
            get { return _driveFromInput; }
            set { _driveFromInput = value; }
        }

        public override void OnInit(CYGWorld world)
        {
            if (!world.TryGetStore(CYGPhysicsBody2D.TypeId, out _))
            {
                world.Register<CYGPhysicsBody2D>();
            }
        }

        public override void OnUpdate(in CYGSystemContext context)
        {
            _physics.FixedDeltaTime = context.DeltaTime;
            SyncBodiesFromEcs(context);
            _physics.Step();
            SyncBodiesToEcs(context.World);
        }

        private void SyncBodiesFromEcs(in CYGSystemContext context)
        {
            CYGWorld world = context.World;
            for (int i = 0; i < world.SlotCount; i++)
            {
                CYGEntityRef entity;
                if (!world.TryGetEntityAt(i, out entity))
                {
                    continue;
                }

                CYGPhysicsBody2D component;
                if (!world.TryGet<CYGPhysicsBody2D>(entity, out component))
                {
                    continue;
                }

                CYGPhysics2DBody body = BuildBody(component);
                if (_driveFromInput && component.PlayerId > 0)
                {
                    CYGPlayerInput input;
                    if (context.TryGetInput(component.PlayerId, out input))
                    {
                        FP inputX = FP.FromFraction(input.MoveX, 1000);
                        FP inputY = FP.FromFraction(input.MoveY, 1000);
                        body.Velocity = new FPVector2(inputX * MoveSpeed, inputY * MoveSpeed);
                        FP groundOffset = (CYGPhysics2DShapeType)component.Shape == CYGPhysics2DShapeType.Circle
                            ? component.Radius
                            : component.HalfExtents.y;
                        if (input.Jump && body.Position.y <= groundOffset)
                        {
                            body.Velocity = new FPVector2(body.Velocity.x, JumpImpulse);
                        }
                        body.IsAwake = true;
                    }
                }

                int bodyId = ResolveBody(world, entity, ref component);
                if (bodyId < 0)
                {
                    bodyId = _physics.AddBody(body);
                    component.BodyId = bodyId;
                    _bodyByEntity[entity] = bodyId;
                    world.Set(entity, component);
                }
                else
                {
                    _physics.UpdateBody(bodyId, body);
                }
            }

            RemoveStaleBodies(world);
        }

        private int ResolveBody(CYGWorld world, CYGEntityRef entity, ref CYGPhysicsBody2D component)
        {
            int mappedId;
            if (_bodyByEntity.TryGetValue(entity, out mappedId))
            {
                CYGPhysics2DBody existing;
                if (_physics.TryGetBody(mappedId, out existing))
                {
                    if (component.BodyId != mappedId)
                    {
                        component.BodyId = mappedId;
                        world.Set(entity, component);
                    }
                    return mappedId;
                }
            }

            CYGPhysics2DBody byId;
            if (component.BodyId >= 0 && _physics.TryGetBody(component.BodyId, out byId))
            {
                _bodyByEntity[entity] = component.BodyId;
                return component.BodyId;
            }

            return -1;
        }

        private void RemoveStaleBodies(CYGWorld world)
        {
            _staleEntities.Clear();
            foreach (KeyValuePair<CYGEntityRef, int> pair in _bodyByEntity)
            {
                CYGPhysicsBody2D component;
                if (!world.TryGet<CYGPhysicsBody2D>(pair.Key, out component))
                {
                    _staleEntities.Add(pair.Key);
                }
            }

            for (int i = 0; i < _staleEntities.Count; i++)
            {
                int bodyId;
                if (_bodyByEntity.TryGetValue(_staleEntities[i], out bodyId))
                {
                    _physics.DestroyBody(bodyId);
                    _bodyByEntity.Remove(_staleEntities[i]);
                }
            }
        }

        private static CYGPhysics2DBody BuildBody(in CYGPhysicsBody2D component)
        {
            return new CYGPhysics2DBody
            {
                Type = (CYGPhysics2DBodyType)component.BodyType,
                Shape = (CYGPhysics2DShapeType)component.Shape,
                Position = component.Position,
                Angle = component.Angle,
                Velocity = component.Velocity,
                AngularVelocity = component.AngularVelocity,
                Radius = component.Radius,
                HalfExtents = component.HalfExtents,
                Mass = component.Mass,
                Friction = component.Friction,
                Restitution = component.Restitution,
                LinearDamping = component.LinearDamping,
                AngularDamping = component.AngularDamping,
                GravityScale = component.GravityScale,
                Layer = component.Layer,
                CollisionMask = component.CollisionMask,
                IsTrigger = component.IsTrigger,
                IsAwake = component.IsAwake,
                SleepFrames = component.SleepFrames,
            };
        }

        private void SyncBodiesToEcs(CYGWorld world)
        {
            for (int i = 0; i < world.SlotCount; i++)
            {
                CYGEntityRef entity;
                if (!world.TryGetEntityAt(i, out entity))
                {
                    continue;
                }

                CYGPhysicsBody2D component;
                if (!world.TryGet<CYGPhysicsBody2D>(entity, out component))
                {
                    continue;
                }

                CYGPhysics2DBody body;
                if (component.BodyId < 0 || !_physics.TryGetBody(component.BodyId, out body))
                {
                    continue;
                }

                component.Position = body.Position;
                component.Angle = body.Angle;
                component.Velocity = body.Velocity;
                component.AngularVelocity = body.AngularVelocity;
                component.IsAwake = body.IsAwake;
                component.SleepFrames = body.SleepFrames;
                world.Set(entity, component);
            }
        }
    }
}
