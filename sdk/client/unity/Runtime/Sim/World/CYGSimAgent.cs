using System;
using System.Collections.Generic;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Built-in movement state for the unified runner. Its negative type id is
    /// reserved for SDK components so it can never collide with DSL-generated
    /// component ids, which start at 1.
    /// </summary>
    public struct CYGSimAgent : ICYGComponent
    {
        public const int TypeId = -1001;
        public const uint SchemaHashValue = 0x8A31C7F2u;
        public const int SizeInBytes = 89;

        public const byte GroundedFlag = 1;
        public const byte AttackingFlag = 2;

        public int PlayerId;
        public FPVector3 Position;
        public FPVector3 Velocity;
        public FPQuaternion Rotation;
        public int Health;
        public byte Flags;

        public bool Grounded
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return (Flags & GroundedFlag) != 0; }
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            set
            {
                if (value)
                {
                    Flags |= GroundedFlag;
                }
                else
                {
                    Flags = (byte)(Flags & ~GroundedFlag);
                }
            }
        }

        /// <summary>
        /// True while the player is holding the primary action button. This is
        /// simulation state so rollback and remote views see the same value;
        /// presentation layers fire triggers on its rising edge.
        /// </summary>
        public bool Attacking
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return (Flags & AttackingFlag) != 0; }
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            set
            {
                if (value)
                {
                    Flags |= AttackingFlag;
                }
                else
                {
                    Flags = (byte)(Flags & ~AttackingFlag);
                }
            }
        }

        public int ComponentTypeId
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return TypeId; }
        }

        public uint ComponentSchemaHash
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return SchemaHashValue; }
        }

        public int WireSize
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return SizeInBytes; }
        }

        public int WriteTo(byte[] buffer, int offset)
        {
            CYGWire.RequireWritable(buffer, offset, SizeInBytes);
            CYGWire.WriteInt32(buffer, offset, PlayerId);
            Position.WriteTo(buffer, offset + 4);
            Velocity.WriteTo(buffer, offset + 28);
            Rotation.WriteTo(buffer, offset + 52);
            CYGWire.WriteInt32(buffer, offset + 84, Health);
            CYGWire.WriteByte(buffer, offset + 88, Flags);
            return SizeInBytes;
        }

        public void ReadFrom(byte[] buffer, int offset)
        {
            CYGWire.RequireReadable(buffer, offset, SizeInBytes);
            PlayerId = CYGWire.ReadInt32(buffer, offset);
            Position.ReadFrom(buffer, offset + 4);
            Velocity.ReadFrom(buffer, offset + 28);
            Rotation.ReadFrom(buffer, offset + 52);
            Health = CYGWire.ReadInt32(buffer, offset + 84);
            Flags = CYGWire.ReadByte(buffer, offset + 88);
        }

        public void ComputeChecksum(ref uint crc)
        {
            CYGChecksum.HashInt(ref crc, PlayerId);
            Position.ComputeChecksum(ref crc);
            Velocity.ComputeChecksum(ref crc);
            Rotation.ComputeChecksum(ref crc);
            CYGChecksum.HashInt(ref crc, Health);
            CYGChecksum.HashByte(ref crc, Flags);
        }

        public static CYGSimAgent Create(int playerId, FPVector3 position, int health = 100)
        {
            return new CYGSimAgent
            {
                PlayerId = playerId,
                Position = position,
                Velocity = FPVector3.Zero,
                Rotation = FPQuaternion.Identity,
                Health = health,
                Flags = GroundedFlag,
            };
        }
    }

    /// <summary>
    /// Default deterministic character movement used when a rollback session
    /// has no custom step. Mirrors the pure kinematic prototype but stores all
    /// state in the ECS world, so snapshots and checksums include it.
    /// </summary>
    public sealed class CYGCharacterMovementSystem : CYGSystem
    {
        public FP MoveSpeed = FP.FromInt(6);
        public FP JumpImpulse = FP.FromInt(8);
        public FP Gravity = FP.FromInt(18);

        /// <summary>
        /// Optional collision world (boxes, capsules and baked static meshes).
        /// When set and it contains meshes, movement uses the deterministic
        /// capsule controller instead of the flat y=0 fallback.
        /// </summary>
        public SimpleCollisionWorld CollisionWorld;

        public FP Radius = FP.Half;
        public FP Height = FP.FromInt(2);

        private readonly Dictionary<CYGEntityRef, FPCharacterController> _controllers =
            new Dictionary<CYGEntityRef, FPCharacterController>();

        public override void OnInit(CYGWorld world)
        {
            if (!world.TryGetStore(CYGSimAgent.TypeId, out _))
            {
                world.Register<CYGSimAgent>();
            }
        }

        public override void OnUpdate(in CYGSystemContext context)
        {
            CYGWorld world = context.World;
            FP dt = context.DeltaTime;
            bool useMeshCollision = CollisionWorld != null && CollisionWorld.Meshes.Count > 0;

            for (int i = 0; i < world.SlotCount; i++)
            {
                CYGEntityRef entity;
                if (!world.TryGetEntityAt(i, out entity))
                {
                    continue;
                }

                CYGSimAgent agent;
                if (!world.TryGet<CYGSimAgent>(entity, out agent))
                {
                    continue;
                }

                CYGPlayerInput input;
                bool hasInput = context.TryGetInput(agent.PlayerId, out input);
                FP inputX = hasInput ? FP.FromFraction(input.MoveX, 1000) : FP.Zero;
                FP inputZ = hasInput ? FP.FromFraction(input.MoveY, 1000) : FP.Zero;

                if (useMeshCollision)
                {
                    UpdateWithCollision(entity, input, hasInput, inputX, inputZ, dt, ref agent);
                }
                else
                {
                    UpdateFlat(input, hasInput, inputX, inputZ, dt, ref agent);
                }

                world.Set(entity, agent);
            }
        }

        private void UpdateFlat(
            in CYGPlayerInput input,
            bool hasInput,
            FP inputX,
            FP inputZ,
            FP dt,
            ref CYGSimAgent agent)
        {
            agent.Attacking = hasInput && input.Action1;

            if (hasInput)
            {
                agent.Velocity.x = inputX * MoveSpeed;
                agent.Velocity.z = inputZ * MoveSpeed;

                if (agent.Grounded && input.Jump)
                {
                    agent.Velocity.y = JumpImpulse;
                }
            }

            agent.Velocity.y -= Gravity * dt;
            agent.Position.x += agent.Velocity.x * dt;
            agent.Position.y += agent.Velocity.y * dt;
            agent.Position.z += agent.Velocity.z * dt;

            if (agent.Position.y <= FP.Zero)
            {
                agent.Position.y = FP.Zero;
                if (agent.Velocity.y < FP.Zero)
                {
                    agent.Velocity.y = FP.Zero;
                }
                agent.Grounded = true;
            }
            else
            {
                agent.Grounded = false;
            }

            UpdateRotation(ref agent);
        }

        private void UpdateWithCollision(
            CYGEntityRef entity,
            in CYGPlayerInput input,
            bool hasInput,
            FP inputX,
            FP inputZ,
            FP dt,
            ref CYGSimAgent agent)
        {
            agent.Attacking = hasInput && input.Action1;

            FPCharacterController controller;
            if (!_controllers.TryGetValue(entity, out controller))
            {
                controller = new FPCharacterController();
                _controllers[entity] = controller;
            }

            controller.Position = agent.Position;
            controller.Velocity = agent.Velocity;
            controller.IsGrounded = agent.Grounded;
            controller.GroundNormal = FPVector3.Up;
            controller.Radius = Radius;
            controller.Height = Height;

            FPVector3 targetVelocity = new FPVector3(inputX * MoveSpeed, FP.Zero, inputZ * MoveSpeed);
            if (hasInput && agent.Grounded && input.Jump)
            {
                controller.IsGrounded = false;
                controller.Velocity.y = JumpImpulse;
            }

            controller.SimpleMove(targetVelocity, dt, CollisionWorld, Gravity);

            agent.Position = controller.Position;
            agent.Velocity = controller.Velocity;
            agent.Grounded = controller.IsGrounded;
            UpdateRotation(ref agent);
        }

        private static void UpdateRotation(ref CYGSimAgent agent)
        {
            if (agent.Velocity.x != FP.Zero || agent.Velocity.z != FP.Zero)
            {
                FP yaw = FP.Atan2(agent.Velocity.x, agent.Velocity.z) * FP.Rad2Deg;
                agent.Rotation = FPQuaternion.Euler(FP.Zero, yaw, FP.Zero);
            }
        }
    }
}
