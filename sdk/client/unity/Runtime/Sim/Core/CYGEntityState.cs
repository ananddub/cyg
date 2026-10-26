using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Deterministic snapshot of an entity's simulation state.
    /// Pure struct, zero heap allocations, packed for optimal cache coherence.
    /// </summary>
    [StructLayout(LayoutKind.Sequential, Pack = 1)]
    public struct CYGEntityState : IEquatable<CYGEntityState>
    {
        /// <summary>Unique runtime identifier of this simulation entity (0 indicates unallocated slot).</summary>
        public int EntityId;

        /// <summary>Fixed-point 3D world position.</summary>
        public FPVector3 Position;

        /// <summary>Fixed-point 3D linear velocity.</summary>
        public FPVector3 Velocity;

        /// <summary>Fixed-point quaternion rotation.</summary>
        public FPQuaternion Rotation;

        /// <summary>Entity health or status points.</summary>
        public int Health;

        public CYGEntityState(int entityId, FPVector3 position, FPVector3 velocity, FPQuaternion rotation, int health)
        {
            EntityId = entityId;
            Position = position;
            Velocity = velocity;
            Rotation = rotation;
            Health = health;
        }

        public CYGEntityState(int entityId, FPVector3 position, int health = 100)
        {
            EntityId = entityId;
            Position = position;
            Velocity = FPVector3.Zero;
            Rotation = FPQuaternion.Identity;
            Health = health;
        }

        /// <summary>True if this slot represents an active entity in the simulation world.</summary>
        public bool IsActive
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => EntityId != 0;
        }

        #region Equality

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool Equals(CYGEntityState other)
        {
            return EntityId == other.EntityId
                && Position.Equals(other.Position)
                && Velocity.Equals(other.Velocity)
                && Rotation.Equals(other.Rotation)
                && Health == other.Health;
        }

        public override bool Equals(object obj) => obj is CYGEntityState other && Equals(other);

        public override int GetHashCode()
        {
            unchecked
            {
                int hash = EntityId;
                hash = (hash * 397) ^ Position.GetHashCode();
                hash = (hash * 397) ^ Velocity.GetHashCode();
                hash = (hash * 397) ^ Rotation.GetHashCode();
                hash = (hash * 397) ^ Health;
                return hash;
            }
        }

        public static bool operator ==(CYGEntityState left, CYGEntityState right) => left.Equals(right);
        public static bool operator !=(CYGEntityState left, CYGEntityState right) => !left.Equals(right);

        public override string ToString()
        {
            return $"EntityState(ID: {EntityId}, Pos: {Position}, Vel: {Velocity}, Rot: {Rotation}, HP: {Health})";
        }

        #endregion
    }
}
