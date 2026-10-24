//-----------------------------------------------------------------------------
// CYGPhysics2DWorldSnapshot.cs
// Complete rollback snapshot for the deterministic 2D physics world.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Immutable snapshot of all simulated 2D physics state: bodies (including
    /// pose, linear/angular velocity and awake state), tick, next body id and the
    /// persistent trigger/collision pair sets used by Enter/Stay/Exit callbacks.
    ///
    /// World configuration (gravity, timestep, iteration counts) and joints are
    /// not captured; rollback callers must keep those unchanged. Restoring the
    /// snapshot reproduces the world checksum exactly.
    /// </summary>
    public sealed class CYGPhysics2DWorldSnapshot
    {
        /// <summary>Simulation tick at capture time.</summary>
        public int Tick { get; }

        /// <summary>Active bodies in stable id order.</summary>
        public CYGPhysics2DBody[] Bodies { get; }

        public int BodyCount
        {
            get { return Bodies.Length; }
        }

        internal int NextBodyId { get; }
        internal CYGPhysics2DPair[] CollisionPairs { get; }
        internal int CollisionCount { get; }
        internal CYGPhysics2DPair[] TriggerPairs { get; }
        internal int TriggerCount { get; }

        internal CYGPhysics2DWorldSnapshot(
            int tick,
            int nextBodyId,
            CYGPhysics2DBody[] bodies,
            CYGPhysics2DPair[] collisionPairs,
            int collisionCount,
            CYGPhysics2DPair[] triggerPairs,
            int triggerCount)
        {
            Tick = tick;
            NextBodyId = nextBodyId;
            Bodies = bodies;
            CollisionPairs = collisionPairs;
            CollisionCount = collisionCount;
            TriggerPairs = triggerPairs;
            TriggerCount = triggerCount;
        }

        /// <summary>Captured body at the given snapshot index.</summary>
        public CYGPhysics2DBody GetBody(int index)
        {
            if ((uint)index >= (uint)Bodies.Length)
            {
                throw new ArgumentOutOfRangeException(nameof(index));
            }

            return Bodies[index];
        }

        /// <summary>
        /// CRC32 using exactly the same layout as
        /// <see cref="CYGPhysics2DWorld.ComputeChecksum"/>.
        /// </summary>
        public uint ComputeChecksum()
        {
            uint crc = 0xFFFFFFFFu;

            CYGChecksum.HashInt(ref crc, Tick);
            CYGChecksum.HashInt(ref crc, Bodies.Length);
            CYGChecksum.HashInt(ref crc, NextBodyId);

            for (int i = 0; i < Bodies.Length; i++)
            {
                Bodies[i].ComputeChecksum(ref crc);
            }

            CYGChecksum.HashInt(ref crc, CollisionCount);
            for (int i = 0; i < CollisionCount; i++)
            {
                CYGChecksum.HashInt(ref crc, CollisionPairs[i].BodyA);
                CYGChecksum.HashInt(ref crc, CollisionPairs[i].BodyB);
            }

            CYGChecksum.HashInt(ref crc, TriggerCount);
            for (int i = 0; i < TriggerCount; i++)
            {
                CYGChecksum.HashInt(ref crc, TriggerPairs[i].BodyA);
                CYGChecksum.HashInt(ref crc, TriggerPairs[i].BodyB);
            }

            return crc ^ 0xFFFFFFFFu;
        }
    }
}
