using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// ECS-owned 3D rigid body state. The component is the authoritative
    /// record: the physics world is rebuilt from it before every step and its
    /// results are written back afterwards, so world snapshots and rollback
    /// restore rigid bodies together with every other component.
    /// </summary>
    public struct CYGPhysicsBody3D : ICYGComponent
    {
        public const int TypeId = -1010;
        public const uint SchemaHashValue = 0xB7C1D3E5u;
        public const int SizeInBytes = 212;

        public const byte ActiveFlag = 1;
        public const byte AwakeFlag = 2;
        public const byte TriggerFlag = 4;
        public const byte CcdFlag = 8;

        public int BodyId;
        public int PlayerId;
        public byte BodyType;
        public byte ShapeType;
        public byte Flags;
        public byte Reserved;
        public FPVector3 Position;
        public FPQuaternion Rotation;
        public FPVector3 LinearVelocity;
        public FPVector3 AngularVelocity;
        public FPVector3 HalfExtents;
        public FP Radius;
        public FP Height;
        public FP Mass;
        public FP Friction;
        public FP Restitution;
        public FP LinearDamping;
        public FP AngularDamping;
        public FP GravityScale;
        public int Layer;
        public uint LayerMask;

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

        public bool EnableCcd
        {
            get { return (Flags & CcdFlag) != 0; }
            set { SetFlag(CcdFlag, value); }
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
            CYGWire.WriteByte(buffer, offset + 9, ShapeType);
            CYGWire.WriteByte(buffer, offset + 10, Flags);
            CYGWire.WriteByte(buffer, offset + 11, Reserved);
            Position.WriteTo(buffer, offset + 12);
            Rotation.WriteTo(buffer, offset + 36);
            LinearVelocity.WriteTo(buffer, offset + 68);
            AngularVelocity.WriteTo(buffer, offset + 92);
            HalfExtents.WriteTo(buffer, offset + 116);
            Radius.WriteTo(buffer, offset + 140);
            Height.WriteTo(buffer, offset + 148);
            Mass.WriteTo(buffer, offset + 156);
            Friction.WriteTo(buffer, offset + 164);
            Restitution.WriteTo(buffer, offset + 172);
            LinearDamping.WriteTo(buffer, offset + 180);
            AngularDamping.WriteTo(buffer, offset + 188);
            GravityScale.WriteTo(buffer, offset + 196);
            CYGWire.WriteInt32(buffer, offset + 204, Layer);
            CYGWire.WriteUInt32(buffer, offset + 208, LayerMask);
            return SizeInBytes;
        }

        public void ReadFrom(byte[] buffer, int offset)
        {
            CYGWire.RequireReadable(buffer, offset, SizeInBytes);
            BodyId = CYGWire.ReadInt32(buffer, offset);
            PlayerId = CYGWire.ReadInt32(buffer, offset + 4);
            BodyType = CYGWire.ReadByte(buffer, offset + 8);
            ShapeType = CYGWire.ReadByte(buffer, offset + 9);
            Flags = CYGWire.ReadByte(buffer, offset + 10);
            Reserved = CYGWire.ReadByte(buffer, offset + 11);
            Position.ReadFrom(buffer, offset + 12);
            Rotation.ReadFrom(buffer, offset + 36);
            LinearVelocity.ReadFrom(buffer, offset + 68);
            AngularVelocity.ReadFrom(buffer, offset + 92);
            HalfExtents.ReadFrom(buffer, offset + 116);
            Radius.ReadFrom(buffer, offset + 140);
            Height.ReadFrom(buffer, offset + 148);
            Mass.ReadFrom(buffer, offset + 156);
            Friction.ReadFrom(buffer, offset + 164);
            Restitution.ReadFrom(buffer, offset + 172);
            LinearDamping.ReadFrom(buffer, offset + 180);
            AngularDamping.ReadFrom(buffer, offset + 188);
            GravityScale.ReadFrom(buffer, offset + 196);
            Layer = CYGWire.ReadInt32(buffer, offset + 204);
            LayerMask = CYGWire.ReadUInt32(buffer, offset + 208);
        }

        public void ComputeChecksum(ref uint crc)
        {
            CYGChecksum.HashInt(ref crc, BodyId);
            CYGChecksum.HashInt(ref crc, PlayerId);
            CYGChecksum.HashByte(ref crc, BodyType);
            CYGChecksum.HashByte(ref crc, ShapeType);
            CYGChecksum.HashByte(ref crc, Flags);
            CYGChecksum.HashByte(ref crc, Reserved);
            Position.ComputeChecksum(ref crc);
            Rotation.ComputeChecksum(ref crc);
            LinearVelocity.ComputeChecksum(ref crc);
            AngularVelocity.ComputeChecksum(ref crc);
            HalfExtents.ComputeChecksum(ref crc);
            Radius.ComputeChecksum(ref crc);
            Height.ComputeChecksum(ref crc);
            Mass.ComputeChecksum(ref crc);
            Friction.ComputeChecksum(ref crc);
            Restitution.ComputeChecksum(ref crc);
            LinearDamping.ComputeChecksum(ref crc);
            AngularDamping.ComputeChecksum(ref crc);
            GravityScale.ComputeChecksum(ref crc);
            CYGChecksum.HashInt(ref crc, Layer);
            CYGChecksum.HashUInt(ref crc, LayerMask);
        }

        public static CYGPhysicsBody3D DynamicBox(int playerId, FPVector3 position, FPVector3 halfExtents, FP mass)
        {
            return Create(playerId, CYGBodyType.Dynamic, CYGShapeType.Box, position, halfExtents, FP.Zero, FP.Zero, mass);
        }

        public static CYGPhysicsBody3D DynamicSphere(int playerId, FPVector3 position, FP radius, FP mass)
        {
            return Create(playerId, CYGBodyType.Dynamic, CYGShapeType.Sphere, position, FPVector3.Zero, radius, FP.Zero, mass);
        }

        public static CYGPhysicsBody3D DynamicCapsule(int playerId, FPVector3 position, FP radius, FP height, FP mass)
        {
            return Create(playerId, CYGBodyType.Dynamic, CYGShapeType.Capsule, position, FPVector3.Zero, radius, height, mass);
        }

        public static CYGPhysicsBody3D StaticBox(FPVector3 position, FPVector3 halfExtents)
        {
            return Create(0, CYGBodyType.Static, CYGShapeType.Box, position, halfExtents, FP.Zero, FP.Zero, FP.Zero);
        }

        public static CYGPhysicsBody3D StaticSphere(FPVector3 position, FP radius)
        {
            return Create(0, CYGBodyType.Static, CYGShapeType.Sphere, position, FPVector3.Zero, radius, FP.Zero, FP.Zero);
        }

        private static CYGPhysicsBody3D Create(
            int playerId,
            CYGBodyType bodyType,
            CYGShapeType shapeType,
            FPVector3 position,
            FPVector3 halfExtents,
            FP radius,
            FP height,
            FP mass)
        {
            return new CYGPhysicsBody3D
            {
                BodyId = -1,
                PlayerId = playerId,
                BodyType = (byte)bodyType,
                ShapeType = (byte)shapeType,
                Flags = ActiveFlag | AwakeFlag,
                Reserved = 0,
                Position = position,
                Rotation = FPQuaternion.Identity,
                LinearVelocity = FPVector3.Zero,
                AngularVelocity = FPVector3.Zero,
                HalfExtents = halfExtents,
                Radius = radius,
                Height = height,
                Mass = mass,
                Friction = FP.FromFraction(1, 2),
                Restitution = FP.Zero,
                LinearDamping = FP.Zero,
                AngularDamping = FP.Zero,
                GravityScale = FP.One,
                Layer = 0,
                LayerMask = 0xFFFFFFFFu,
            };
        }
    }

    /// <summary>
    /// Steps a <see cref="CYGPhysicsWorld"/> from <see cref="CYGPhysicsBody3D"/>
    /// components. Bodies are matched to entities by slot order; body ids are
    /// cached in the component and re-used after rollback.
    ///
    /// The bridge is stateless between ticks: contact impulses, pair history
    /// and sleeping are disabled so a restored ECS snapshot produces exactly
    /// the same step as the original simulation. Physics entities should be
    /// spawned before the first tick; late structural changes may produce body
    /// ids that differ between clients.
    /// </summary>
    public sealed class CYGPhysics3DSystem : CYGSystem
    {
        private readonly CYGPhysicsWorld _physics;
        private readonly Dictionary<CYGEntityRef, int> _bodyByEntity = new Dictionary<CYGEntityRef, int>();
        private readonly List<CYGEntityRef> _staleEntities = new List<CYGEntityRef>();
        private bool _driveFromInput = true;

        public CYGPhysics3DSystem(CYGPhysicsSettings settings = null)
        {
            _physics = new CYGPhysicsWorld(settings ?? new CYGPhysicsSettings());
            _physics.Settings.EnableSleeping = false;
        }

        public CYGPhysicsWorld Physics
        {
            get { return _physics; }
        }

        /// <summary>
        /// Adds baked level geometry to the physics world as a static mesh body.
        /// Meshes are deterministic content, so they are added before the first
        /// tick and are not part of the ECS rollback snapshot.
        /// </summary>
        public int AddStaticMesh(CYGStaticMesh mesh)
        {
            return _physics.AddStaticMesh(mesh);
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
            if (!world.TryGetStore(CYGPhysicsBody3D.TypeId, out _))
            {
                world.Register<CYGPhysicsBody3D>();
            }
        }

        public override void OnUpdate(in CYGSystemContext context)
        {
            SyncBodiesFromEcs(context);
            _physics.ResetTransientState();
            _physics.Step(context.DeltaTime);
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

                CYGPhysicsBody3D component;
                if (!world.TryGet<CYGPhysicsBody3D>(entity, out component))
                {
                    continue;
                }

                CYGPhysicsBody body = ResolveBody(world, entity, ref component);
                if (body == null)
                {
                    continue;
                }

                body.Type = (CYGBodyType)component.BodyType;
                body.Collider = new CYGCollider
                {
                    Shape = BuildShape(component),
                    Offset = FPVector3.Zero,
                    IsTrigger = component.IsTrigger,
                };
                if (body.Mass != component.Mass || body.Type == CYGBodyType.Dynamic && body.Inertia == FP.Zero)
                {
                    body.Mass = component.Mass;
                    body.RecalculateInertia();
                }
                body.Friction = component.Friction;
                body.Restitution = component.Restitution;
                body.LinearDamping = component.LinearDamping;
                body.AngularDamping = component.AngularDamping;
                body.GravityScale = component.GravityScale;
                body.Layer = component.Layer;
                body.LayerMask = component.LayerMask;
                body.EnableCcd = component.EnableCcd;
                body.IsActive = component.IsActive;
                body.IsAwake = component.IsAwake;
                body.Position = component.Position;
                body.Rotation = component.Rotation;
                body.LinearVelocity = component.LinearVelocity;
                body.AngularVelocity = component.AngularVelocity;

                if (_driveFromInput && component.PlayerId > 0)
                {
                    CYGPlayerInput input;
                    if (context.TryGetInput(component.PlayerId, out input))
                    {
                        FP inputX = FP.FromFraction(input.MoveX, 1000);
                        FP inputZ = FP.FromFraction(input.MoveY, 1000);
                        body.LinearVelocity = new FPVector3(inputX * MoveSpeed, body.LinearVelocity.y, inputZ * MoveSpeed);
                        if (input.Jump && body.Position.y <= component.Radius)
                        {
                            body.LinearVelocity = new FPVector3(body.LinearVelocity.x, JumpImpulse, body.LinearVelocity.z);
                        }
                        body.IsAwake = true;
                    }
                }
            }

            RemoveStaleBodies(world);
        }

        private void RemoveStaleBodies(CYGWorld world)
        {
            _staleEntities.Clear();
            foreach (KeyValuePair<CYGEntityRef, int> pair in _bodyByEntity)
            {
                CYGPhysicsBody3D component;
                if (!world.TryGet<CYGPhysicsBody3D>(pair.Key, out component))
                {
                    _staleEntities.Add(pair.Key);
                }
            }

            for (int i = 0; i < _staleEntities.Count; i++)
            {
                int bodyId;
                if (_bodyByEntity.TryGetValue(_staleEntities[i], out bodyId))
                {
                    _physics.RemoveBody(bodyId);
                    _bodyByEntity.Remove(_staleEntities[i]);
                }
            }
        }

        private CYGPhysicsBody ResolveBody(CYGWorld world, CYGEntityRef entity, ref CYGPhysicsBody3D component)
        {
            CYGPhysicsBody body = component.BodyId >= 0 ? _physics.GetBody(component.BodyId) : null;
            int mappedId;
            if (body == null && _bodyByEntity.TryGetValue(entity, out mappedId))
            {
                body = _physics.GetBody(mappedId);
                if (body != null)
                {
                    component.BodyId = mappedId;
                    world.Set(entity, component);
                }
            }

            if (body != null)
            {
                return body;
            }

            body = CreateBody(component);
            int id = _physics.AddBody(body);
            component.BodyId = id;
            _bodyByEntity[entity] = id;
            world.Set(entity, component);
            return body;
        }

        private static CYGPhysicsBody CreateBody(in CYGPhysicsBody3D component)
        {
            CYGCollider collider = new CYGCollider
            {
                Shape = BuildShape(component),
                Offset = FPVector3.Zero,
                IsTrigger = component.IsTrigger,
            };

            CYGPhysicsBody body;
            CYGBodyType type = (CYGBodyType)component.BodyType;
            if (type == CYGBodyType.Dynamic)
            {
                body = CYGPhysicsBody.CreateDynamic(collider, component.Mass > FP.Zero ? component.Mass : FP.One);
            }
            else if (type == CYGBodyType.Kinematic)
            {
                body = CYGPhysicsBody.CreateKinematic(collider);
            }
            else
            {
                body = CYGPhysicsBody.CreateStatic(collider);
            }

            body.Friction = component.Friction;
            body.Restitution = component.Restitution;
            body.LinearDamping = component.LinearDamping;
            body.AngularDamping = component.AngularDamping;
            body.GravityScale = component.GravityScale;
            body.Layer = component.Layer;
            body.LayerMask = component.LayerMask;
            body.EnableCcd = component.EnableCcd;
            body.IsAwake = component.IsAwake;
            body.Position = component.Position;
            body.Rotation = component.Rotation;
            body.LinearVelocity = component.LinearVelocity;
            body.AngularVelocity = component.AngularVelocity;
            return body;
        }

        private static CYGShape BuildShape(in CYGPhysicsBody3D component)
        {
            switch ((CYGShapeType)component.ShapeType)
            {
                case CYGShapeType.Sphere:
                    return CYGShape.CreateSphere(component.Radius);
                case CYGShapeType.Capsule:
                    return CYGShape.CreateCapsule(component.Radius, component.Height);
                default:
                    return CYGShape.CreateBox(component.HalfExtents);
            }
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

                CYGPhysicsBody3D component;
                if (!world.TryGet<CYGPhysicsBody3D>(entity, out component))
                {
                    continue;
                }

                CYGPhysicsBody body = component.BodyId >= 0 ? _physics.GetBody(component.BodyId) : null;
                if (body == null)
                {
                    continue;
                }

                component.Position = body.Position;
                component.Rotation = body.Rotation;
                component.LinearVelocity = body.LinearVelocity;
                component.AngularVelocity = body.AngularVelocity;
                component.IsAwake = body.IsAwake;
                world.Set(entity, component);
            }
        }
    }
}
