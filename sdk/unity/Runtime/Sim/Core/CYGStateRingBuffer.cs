using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Fixed-size circular array storing full world state snapshots per simulation tick.
    /// Operates with Zero GC allocations by recycling preallocated struct buffers.
    /// Used for instant state restoration during rollback resimulation.
    /// </summary>
    public sealed class CYGStateRingBuffer
    {
        public const int DefaultCapacity = 128;   // Standard 128 ticks (~2.1 seconds @ 60Hz)
        public const int DefaultMaxEntities = 256; // Max tracked deterministic entities

        public struct Snapshot
        {
            public int Tick;
            public int EntityCount;
            public uint Checksum;
            public bool IsValid;
            public CYGEntityState[] Entities; // Pre-allocated fixed buffer, never reallocated
        }

        private readonly int _capacity;
        private readonly int _mask;
        private readonly int _maxEntities;
        private readonly Snapshot[] _snapshots;

        private int _earliestTick = -1;
        private int _latestTick = -1;

        public int Capacity => _capacity;
        public int MaxEntities => _maxEntities;
        public int EarliestTick => _earliestTick;
        public int LatestTick => _latestTick;

        public CYGStateRingBuffer(int capacity = DefaultCapacity, int maxEntities = DefaultMaxEntities)
        {
            if (capacity < 16 || (capacity & (capacity - 1)) != 0)
                throw new ArgumentException("Capacity must be a power of two (e.g. 64, 128, 256)", nameof(capacity));

            _capacity = capacity;
            _mask = capacity - 1;
            _maxEntities = maxEntities;

            _snapshots = new Snapshot[_capacity];
            for (int i = 0; i < _capacity; i++)
            {
                _snapshots[i] = new Snapshot
                {
                    Tick = -1,
                    EntityCount = 0,
                    Checksum = 0,
                    IsValid = false,
                    Entities = new CYGEntityState[_maxEntities]
                };
            }
        }

        /// <summary>
        /// Saves the simulation state for a tick. Copies entities into preallocated slot. Zero GC.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void SaveSnapshot(int tick, CYGEntityState[] entities, int count, uint checksum = 0)
        {
            if (count > _maxEntities)
                throw new ArgumentOutOfRangeException(nameof(count), $"Count ({count}) exceeds MaxEntities ({_maxEntities})");

            int slot = tick & _mask;
            ref Snapshot snapshot = ref _snapshots[slot];

            snapshot.Tick = tick;
            snapshot.EntityCount = count;
            snapshot.Checksum = checksum;
            snapshot.IsValid = true;

            if (count > 0)
            {
                Array.Copy(entities, 0, snapshot.Entities, 0, count);
            }

            if (_earliestTick == -1 || tick < _earliestTick)
                _earliestTick = tick;

            if (tick > _latestTick)
                _latestTick = tick;

            // Update earliest tick boundary when ring buffer wraps around
            if (_latestTick - _earliestTick >= _capacity)
            {
                _earliestTick = _latestTick - _capacity + 1;
            }
        }

        /// <summary>
        /// Restores state for a past tick into the target entity array. Zero GC.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool TryRestoreSnapshot(int tick, CYGEntityState[] targetEntities, out int outEntityCount, out uint outChecksum)
        {
            int slot = tick & _mask;
            ref readonly Snapshot snapshot = ref _snapshots[slot];

            if (!snapshot.IsValid || snapshot.Tick != tick)
            {
                outEntityCount = 0;
                outChecksum = 0;
                return false;
            }

            outEntityCount = snapshot.EntityCount;
            outChecksum = snapshot.Checksum;

            if (outEntityCount > 0)
            {
                Array.Copy(snapshot.Entities, 0, targetEntities, 0, outEntityCount);
            }

            return true;
        }

        /// <summary>
        /// Checks if a snapshot exists and is valid for the specified tick.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool HasSnapshot(int tick)
        {
            int slot = tick & _mask;
            ref readonly Snapshot snapshot = ref _snapshots[slot];
            return snapshot.IsValid && snapshot.Tick == tick;
        }

        /// <summary>
        /// Reads snapshot reference without modifying destination buffers. Zero GC.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool TryGetSnapshotRef(int tick, out Snapshot snapshot)
        {
            int slot = tick & _mask;
            snapshot = _snapshots[slot];
            return snapshot.IsValid && snapshot.Tick == tick;
        }

        /// <summary>
        /// Returns the stored checksum for a tick, or 0 if snapshot does not exist.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public uint GetChecksum(int tick)
        {
            int slot = tick & _mask;
            ref readonly Snapshot snapshot = ref _snapshots[slot];
            return (snapshot.IsValid && snapshot.Tick == tick) ? snapshot.Checksum : 0;
        }

        /// <summary>
        /// Clears all stored snapshots and resets tick range.
        /// </summary>
        public void Clear()
        {
            for (int i = 0; i < _capacity; i++)
            {
                _snapshots[i].Tick = -1;
                _snapshots[i].EntityCount = 0;
                _snapshots[i].Checksum = 0;
                _snapshots[i].IsValid = false;
            }
            _earliestTick = -1;
            _latestTick = -1;
        }
    }
}
