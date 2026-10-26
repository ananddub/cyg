//-----------------------------------------------------------------------------
// CYGPhysicsSnapshot.cs
// Bit-exact capture/restore of deterministic 3D physics state and CRC32 hashing.
// Zero UnityEngine dependencies.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Immutable snapshot of solver-visible world state: tick, active body poses,
    /// velocities, sleeping state/timers and active contact/trigger pair keys.
    /// Restoring requires an unchanged body set (same active ids); otherwise an
    /// exception is thrown.
    /// </summary>
    [Serializable]
    public sealed class CYGPhysicsSnapshot
    {
        internal int SnapshotTick;
        internal int[] BodyIds;
        internal CYGBodyType[] BodyTypes;
        internal FPVector3[] Positions;
        internal FPQuaternion[] Rotations;
        internal FPVector3[] LinearVelocities;
        internal FPVector3[] AngularVelocities;
        internal bool[] AwakeStates;
        internal FP[] SleepTimers;
        internal long[] ContactKeys;
        internal long[] TriggerKeys;
        internal CYGPhysicsWorld.CYGWarmManifold[] WarmManifolds;

        private CYGPhysicsSnapshot()
        {
        }

        internal static CYGPhysicsSnapshot Create(
            int tick,
            int[] bodyIds,
            CYGBodyType[] bodyTypes,
            FPVector3[] positions,
            FPQuaternion[] rotations,
            FPVector3[] linearVelocities,
            FPVector3[] angularVelocities,
            bool[] awakeStates,
            FP[] sleepTimers,
            long[] contactKeys,
            long[] triggerKeys,
            CYGPhysicsWorld.CYGWarmManifold[] warmManifolds)
        {
            return new CYGPhysicsSnapshot
            {
                SnapshotTick = tick,
                BodyIds = bodyIds,
                BodyTypes = bodyTypes,
                Positions = positions,
                Rotations = rotations,
                LinearVelocities = linearVelocities,
                AngularVelocities = angularVelocities,
                AwakeStates = awakeStates,
                SleepTimers = sleepTimers,
                ContactKeys = contactKeys,
                TriggerKeys = triggerKeys,
                WarmManifolds = warmManifolds
            };
        }

        /// <summary>Simulation tick at which this snapshot was captured.</summary>
        public int Tick => SnapshotTick;

        public int BodyCount => BodyIds.Length;

        public int ContactPairCount => ContactKeys.Length;

        public int TriggerPairCount => TriggerKeys.Length;

        public int GetBodyId(int index)
        {
            if ((uint)index >= (uint)BodyIds.Length)
                throw new ArgumentOutOfRangeException(nameof(index));
            return BodyIds[index];
        }

        public FPVector3 GetPosition(int index)
        {
            if ((uint)index >= (uint)Positions.Length)
                throw new ArgumentOutOfRangeException(nameof(index));
            return Positions[index];
        }

        public FPQuaternion GetRotation(int index)
        {
            if ((uint)index >= (uint)Rotations.Length)
                throw new ArgumentOutOfRangeException(nameof(index));
            return Rotations[index];
        }

        public FPVector3 GetLinearVelocity(int index)
        {
            if ((uint)index >= (uint)LinearVelocities.Length)
                throw new ArgumentOutOfRangeException(nameof(index));
            return LinearVelocities[index];
        }

        public FPVector3 GetAngularVelocity(int index)
        {
            if ((uint)index >= (uint)AngularVelocities.Length)
                throw new ArgumentOutOfRangeException(nameof(index));
            return AngularVelocities[index];
        }

        public bool GetAwake(int index)
        {
            if ((uint)index >= (uint)AwakeStates.Length)
                throw new ArgumentOutOfRangeException(nameof(index));
            return AwakeStates[index];
        }

        public FP GetSleepTimer(int index)
        {
            if ((uint)index >= (uint)SleepTimers.Length)
                throw new ArgumentOutOfRangeException(nameof(index));
            return SleepTimers[index];
        }

        /// <summary>
        /// CRC32 with exactly the same layout as
        /// <see cref="CYGPhysicsWorld.ComputeChecksum"/>.
        /// </summary>
        public uint ComputeChecksum()
        {
            uint crc = CYGPhysicsHash.Seed;
            CYGChecksum.HashInt(ref crc, SnapshotTick);
            CYGChecksum.HashInt(ref crc, BodyIds.Length);

            for (int i = 0; i < BodyIds.Length; i++)
            {
                CYGPhysicsHash.HashBody(
                    ref crc,
                    BodyIds[i],
                    BodyTypes[i],
                    Positions[i],
                    Rotations[i],
                    LinearVelocities[i],
                    AngularVelocities[i],
                    AwakeStates[i] ? 1 : 0);
            }

            return CYGPhysicsHash.Finish(crc);
        }

        public override string ToString()
        {
            return "CYGPhysicsSnapshot(Tick:" + SnapshotTick + ", Bodies:" + BodyIds.Length +
                   ", Contacts:" + ContactKeys.Length + ", Triggers:" + TriggerKeys.Length +
                   ", Checksum:" + CYGChecksum.ToHexString(ComputeChecksum()) + ")";
        }
    }

    internal static class CYGPhysicsHash
    {
        public const uint Seed = 0xFFFFFFFFu;

        public static uint Finish(uint crc)
        {
            return crc ^ 0xFFFFFFFFu;
        }

        public static void HashBody(ref uint crc, CYGPhysicsBody body)
        {
            HashBody(
                ref crc,
                body.Id,
                body.Type,
                body.Position,
                body.Rotation,
                body.LinearVelocity,
                body.AngularVelocity,
                body.IsAwake ? 1 : 0);
        }

        public static void HashBody(
            ref uint crc,
            int id,
            CYGBodyType type,
            FPVector3 position,
            FPQuaternion rotation,
            FPVector3 linearVelocity,
            FPVector3 angularVelocity,
            int awake)
        {
            CYGChecksum.HashInt(ref crc, id);
            CYGChecksum.HashInt(ref crc, (int)type);
            CYGChecksum.HashLong(ref crc, position.x.RawValue);
            CYGChecksum.HashLong(ref crc, position.y.RawValue);
            CYGChecksum.HashLong(ref crc, position.z.RawValue);
            CYGChecksum.HashLong(ref crc, rotation.x.RawValue);
            CYGChecksum.HashLong(ref crc, rotation.y.RawValue);
            CYGChecksum.HashLong(ref crc, rotation.z.RawValue);
            CYGChecksum.HashLong(ref crc, rotation.w.RawValue);
            CYGChecksum.HashLong(ref crc, linearVelocity.x.RawValue);
            CYGChecksum.HashLong(ref crc, linearVelocity.y.RawValue);
            CYGChecksum.HashLong(ref crc, linearVelocity.z.RawValue);
            CYGChecksum.HashLong(ref crc, angularVelocity.x.RawValue);
            CYGChecksum.HashLong(ref crc, angularVelocity.y.RawValue);
            CYGChecksum.HashLong(ref crc, angularVelocity.z.RawValue);
            CYGChecksum.HashInt(ref crc, awake);
        }
    }
}

namespace CookYourGame.Sim
{
    public sealed partial class CYGPhysicsWorld
    {
        /// <summary>
        /// Captures solver-visible state. Body configuration (shape, mass, layers,
        /// joints) is owned by the world and is not copied.
        /// </summary>
        public CYGPhysicsSnapshot CaptureSnapshot()
        {
            int count = _activeBodyCount;
            int[] ids = new int[count];
            CYGBodyType[] types = new CYGBodyType[count];
            FPVector3[] positions = new FPVector3[count];
            FPQuaternion[] rotations = new FPQuaternion[count];
            FPVector3[] linearVelocities = new FPVector3[count];
            FPVector3[] angularVelocities = new FPVector3[count];
            bool[] awakeStates = new bool[count];
            FP[] sleepTimers = new FP[count];

            int index = 0;
            for (int id = 0; id < _bodies.Count; id++)
            {
                CYGPhysicsBody body = _bodies[id];
                if (body == null || !body.IsActive)
                    continue;

                ids[index] = id;
                types[index] = body.Type;
                positions[index] = body.Position;
                rotations[index] = body.Rotation;
                linearVelocities[index] = body.LinearVelocity;
                angularVelocities[index] = body.AngularVelocity;
                awakeStates[index] = body.IsAwake;
                sleepTimers[index] = body.SleepTimer;
                index++;
            }

            long[] contactKeys = new long[_prevContactKeyCount];
            if (_prevContactKeyCount > 0)
                Array.Copy(_prevContactKeys, contactKeys, _prevContactKeyCount);

            long[] triggerKeys = new long[_prevTriggerKeyCount];
            if (_prevTriggerKeyCount > 0)
                Array.Copy(_prevTriggerKeys, triggerKeys, _prevTriggerKeyCount);

            CYGPhysicsWorld.CYGWarmManifold[] warmManifolds = new CYGPhysicsWorld.CYGWarmManifold[_warmManifoldCount];
            if (_warmManifoldCount > 0)
                Array.Copy(_warmManifolds, warmManifolds, _warmManifoldCount);

            return CYGPhysicsSnapshot.Create(
                Tick,
                ids,
                types,
                positions,
                rotations,
                linearVelocities,
                angularVelocities,
                awakeStates,
                sleepTimers,
                contactKeys,
                triggerKeys,
                warmManifolds);
        }

        /// <summary>
        /// Restores a snapshot produced by <see cref="CaptureSnapshot"/> on this
        /// world. The active body id set and their deterministic order must match.
        /// </summary>
        public void RestoreSnapshot(CYGPhysicsSnapshot snapshot)
        {
            if (snapshot == null)
                throw new ArgumentNullException(nameof(snapshot));
            if (snapshot.BodyCount != _activeBodyCount)
                throw new InvalidOperationException("Snapshot body count does not match the active body set.");

            int index = 0;
            for (int id = 0; id < _bodies.Count; id++)
            {
                CYGPhysicsBody body = _bodies[id];
                if (body == null || !body.IsActive)
                    continue;

                if (index >= snapshot.BodyCount || snapshot.BodyIds[index] != id)
                    throw new InvalidOperationException("Snapshot body ids do not match the active body set.");

                body.Position = snapshot.Positions[index];
                body.Rotation = snapshot.Rotations[index];
                body.LinearVelocity = snapshot.LinearVelocities[index];
                body.AngularVelocity = snapshot.AngularVelocities[index];
                body.IsAwake = snapshot.AwakeStates[index];
                body.SleepTimer = snapshot.SleepTimers[index];
                index++;
            }

            if (index != snapshot.BodyCount)
                throw new InvalidOperationException("Snapshot body count does not match the active body set.");

            Tick = snapshot.Tick;

            EnsureArraySize(ref _prevContactKeys, snapshot.ContactKeys.Length);
            if (snapshot.ContactKeys.Length > 0)
                Array.Copy(snapshot.ContactKeys, _prevContactKeys, snapshot.ContactKeys.Length);
            _prevContactKeyCount = snapshot.ContactKeys.Length;

            EnsureArraySize(ref _prevTriggerKeys, snapshot.TriggerKeys.Length);
            if (snapshot.TriggerKeys.Length > 0)
                Array.Copy(snapshot.TriggerKeys, _prevTriggerKeys, snapshot.TriggerKeys.Length);
            _prevTriggerKeyCount = snapshot.TriggerKeys.Length;

            EnsureArraySize(ref _warmManifolds, snapshot.WarmManifolds.Length);
            if (snapshot.WarmManifolds.Length > 0)
                Array.Copy(snapshot.WarmManifolds, _warmManifolds, snapshot.WarmManifolds.Length);
            _warmManifoldCount = snapshot.WarmManifolds.Length;

            _contactKeyCount = 0;
            _triggerKeyCount = 0;
            _manifoldCount = 0;
            _pairCount = 0;
            _orderCount = 0;
            _events.Clear();
        }
    }
}
