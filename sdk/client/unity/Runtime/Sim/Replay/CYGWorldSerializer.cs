using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Full-state savegame codec for a <see cref="CYGWorld"/>. Writes slot
    /// generations, the free list, RNG state, and every registered component
    /// store in registration order. Events, signals, and commands are session
    /// data and are intentionally not saved.
    /// </summary>
    public static class CYGWorldSerializer
    {
        public const byte SaveVersion = 1;
        public const int HeaderSize = 41;
        public const int StoreHeaderSize = 16;

        public static int ComputeSaveSize(CYGWorld world)
        {
            if (world == null)
            {
                throw new ArgumentNullException(nameof(world));
            }

            return ComputeSaveSize(world.CreateSnapshot());
        }

        /// <summary>Size in bytes of the savegame representing an already captured snapshot.</summary>
        public static int ComputeSaveSize(CYGWorldSnapshot snapshot)
        {
            if (snapshot == null)
            {
                throw new ArgumentNullException(nameof(snapshot));
            }

            long size = HeaderSize
                + (long)snapshot.NextIndex * 5
                + (long)snapshot.FreeCount * 4;

            int bitmapBytes = BitmapBytes(snapshot.NextIndex);
            for (int i = 0; i < snapshot.Stores.Count; i++)
            {
                CYGComponentStoreSnapshot store = snapshot.Stores[i];
                if (store == null)
                {
                    throw new InvalidOperationException("Snapshot store " + i + " has not been captured.");
                }
                size += StoreHeaderSize
                    + bitmapBytes
                    + (long)store.Count * store.ComponentSize;
            }

            if (size > int.MaxValue)
            {
                throw new InvalidOperationException("World savegame exceeds the maximum supported size.");
            }
            return (int)size;
        }

        public static int WriteTo(CYGWorld world, int tick, byte[] buffer, int offset)
        {
            if (world == null)
            {
                throw new ArgumentNullException(nameof(world));
            }

            return WriteSnapshotTo(world.CreateSnapshot(), tick, buffer, offset);
        }

        /// <summary>
        /// Writes a savegame from a world snapshot without touching a live
        /// world. Replay re-anchoring uses this to re-serialize a keyframe.
        /// </summary>
        public static int WriteSnapshotTo(
            CYGWorldSnapshot snapshot,
            int tick,
            byte[] buffer,
            int offset)
        {
            if (snapshot == null)
            {
                throw new ArgumentNullException(nameof(snapshot));
            }
            if (tick < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(tick));
            }

            int size = ComputeSaveSize(snapshot);
            CYGWire.RequireWritable(buffer, offset, size);

            int written = 0;
            CYGWire.WriteByte(buffer, offset + written, SaveVersion);
            written += 1;
            CYGWire.WriteInt32(buffer, offset + written, tick);
            written += 4;
            CYGWire.WriteInt32(buffer, offset + written, snapshot.Count);
            written += 4;
            CYGWire.WriteInt32(buffer, offset + written, snapshot.NextIndex);
            written += 4;
            CYGWire.WriteUInt32(buffer, offset + written, snapshot.StructuralVersion);
            written += 4;
            CYGWire.WriteUInt64(buffer, offset + written, snapshot.Rng.State);
            written += 8;
            CYGWire.WriteUInt64(buffer, offset + written, snapshot.Rng.Increment);
            written += 8;
            CYGWire.WriteInt32(buffer, offset + written, snapshot.FreeCount);
            written += 4;
            CYGWire.WriteInt32(buffer, offset + written, snapshot.Stores.Count);
            written += 4;

            for (int i = 0; i < snapshot.NextIndex; i++)
            {
                CYGWire.WriteInt32(buffer, offset + written, snapshot.Versions[i]);
                written += 4;
                CYGWire.WriteByte(buffer, offset + written, snapshot.Alive[i] ? (byte)1 : (byte)0);
                written += 1;
            }

            for (int i = 0; i < snapshot.FreeCount; i++)
            {
                CYGWire.WriteInt32(buffer, offset + written, snapshot.Free[i]);
                written += 4;
            }

            int bitmapBytes = BitmapBytes(snapshot.NextIndex);
            for (int storeIndex = 0; storeIndex < snapshot.Stores.Count; storeIndex++)
            {
                CYGComponentStoreSnapshot store = snapshot.Stores[storeIndex];
                CYGWire.WriteInt32(buffer, offset + written, store.TypeId);
                written += 4;
                CYGWire.WriteUInt32(buffer, offset + written, store.SchemaHash);
                written += 4;
                CYGWire.WriteInt32(buffer, offset + written, store.ComponentSize);
                written += 4;
                CYGWire.WriteInt32(buffer, offset + written, store.Count);
                written += 4;

                int bitmapStart = offset + written;
                for (int i = 0; i < bitmapBytes; i++)
                {
                    CYGWire.WriteByte(buffer, bitmapStart + i, 0);
                }
                written += bitmapBytes;

                for (int slot = 0; slot < snapshot.NextIndex; slot++)
                {
                    if (!store.IsPresent(slot))
                    {
                        continue;
                    }
                    CYGWire.WriteByte(
                        buffer,
                        bitmapStart + (slot >> 3),
                        (byte)(CYGWire.ReadByte(buffer, bitmapStart + (slot >> 3)) | (1 << (slot & 7))));
                    store.CopyComponentToBuffer(slot, buffer, offset + written);
                    written += store.ComponentSize;
                }
            }

            return written;
        }

        /// <summary>
        /// Restores a savegame into a world with the same component store
        /// registration order and schemas. Returns the restored checksum.
        /// </summary>
        public static uint ReadFrom(
            CYGWorld world,
            byte[] buffer,
            int offset,
            int length,
            out int tick)
        {
            if (world == null)
            {
                throw new ArgumentNullException(nameof(world));
            }
            CYGWire.RequireReadable(buffer, offset, length);
            if (length < HeaderSize)
            {
                throw new InvalidOperationException("Savegame is truncated.");
            }

            int read = 0;
            byte version = CYGWire.ReadByte(buffer, offset + read);
            read += 1;
            if (version != SaveVersion)
            {
                throw new InvalidOperationException("Unsupported savegame version " + version + ".");
            }

            tick = CYGWire.ReadInt32(buffer, offset + read);
            read += 4;
            int entityCount = CYGWire.ReadInt32(buffer, offset + read);
            read += 4;
            int nextIndex = CYGWire.ReadInt32(buffer, offset + read);
            read += 4;
            uint structuralVersion = CYGWire.ReadUInt32(buffer, offset + read);
            read += 4;
            CYGRngState rng = default(CYGRngState);
            rng.State = CYGWire.ReadUInt64(buffer, offset + read);
            read += 8;
            rng.Increment = CYGWire.ReadUInt64(buffer, offset + read);
            read += 8;
            int freeCount = CYGWire.ReadInt32(buffer, offset + read);
            read += 4;
            int storeCount = CYGWire.ReadInt32(buffer, offset + read);
            read += 4;

            if (tick < 0 || entityCount < 0 || nextIndex < 0 || freeCount < 0 || storeCount < 0)
            {
                throw new InvalidOperationException("Savegame header contains negative values.");
            }
            if (storeCount != world.Stores.Count)
            {
                throw new InvalidOperationException(
                    "Savegame has " + storeCount + " stores but the world has " + world.Stores.Count + ".");
            }
            long slotBytes = (long)nextIndex * 5;
            long freeBytes = (long)freeCount * 4;
            if (slotBytes + freeBytes > length - HeaderSize)
            {
                throw new InvalidOperationException("Savegame slot data exceeds the payload.");
            }

            CYGWorldSnapshot snapshot = new CYGWorldSnapshot();
            snapshot.EnsureSlotCapacity(nextIndex);
            int aliveCount = 0;
            for (int i = 0; i < nextIndex; i++)
            {
                snapshot.Versions[i] = CYGWire.ReadInt32(buffer, offset + read);
                read += 4;
                byte alive = CYGWire.ReadByte(buffer, offset + read);
                read += 1;
                if (alive > 1)
                {
                    throw new InvalidOperationException("Savegame slot flag is invalid.");
                }
                snapshot.Alive[i] = alive == 1;
                if (alive == 1)
                {
                    aliveCount++;
                }
            }
            if (aliveCount != entityCount)
            {
                throw new InvalidOperationException("Savegame entity count does not match live slots.");
            }

            snapshot.EnsureFreeCapacity(freeCount);
            for (int i = 0; i < freeCount; i++)
            {
                int slot = CYGWire.ReadInt32(buffer, offset + read);
                read += 4;
                if ((uint)slot >= (uint)nextIndex)
                {
                    throw new InvalidOperationException("Savegame free-list slot is out of range.");
                }
                snapshot.Free[i] = slot;
            }

            snapshot.NextIndex = nextIndex;
            snapshot.Count = entityCount;
            snapshot.StructuralVersion = structuralVersion;
            snapshot.Rng = rng;
            snapshot.FreeCount = freeCount;

            int bitmapBytes = BitmapBytes(nextIndex);
            for (int storeIndex = 0; storeIndex < storeCount; storeIndex++)
            {
                ICYGComponentStore registered = world.Stores[storeIndex];
                if (length - read < StoreHeaderSize)
                {
                    throw new InvalidOperationException("Savegame store header is truncated.");
                }

                int typeId = CYGWire.ReadInt32(buffer, offset + read);
                read += 4;
                uint schemaHash = CYGWire.ReadUInt32(buffer, offset + read);
                read += 4;
                int componentSize = CYGWire.ReadInt32(buffer, offset + read);
                read += 4;
                int componentCount = CYGWire.ReadInt32(buffer, offset + read);
                read += 4;

                if (typeId != registered.TypeId
                    || schemaHash != registered.SchemaHash
                    || componentSize != registered.ComponentSize)
                {
                    throw new InvalidOperationException(
                        "Savegame store " + typeId + "/0x" + schemaHash.ToString("X8")
                        + " does not match registered store " + registered.TypeId
                        + "/0x" + registered.SchemaHash.ToString("X8") + ".");
                }
                if (componentCount < 0 || componentCount > nextIndex)
                {
                    throw new InvalidOperationException("Savegame component count is out of range.");
                }
                if (length - read < bitmapBytes)
                {
                    throw new InvalidOperationException("Savegame presence bitmap is truncated.");
                }

                int bitmapStart = offset + read;
                read += bitmapBytes;
                CYGComponentStoreSnapshot storeSnapshot = registered.CreateSnapshot(nextIndex);
                storeSnapshot.PrepareForLoad(nextIndex);

                int present = 0;
                for (int slot = 0; slot < nextIndex; slot++)
                {
                    bool hasComponent = (CYGWire.ReadByte(buffer, bitmapStart + (slot >> 3)) & (1 << (slot & 7))) != 0;
                    if (!hasComponent)
                    {
                        continue;
                    }
                    if (length - read < componentSize)
                    {
                        throw new InvalidOperationException("Savegame component payload is truncated.");
                    }

                    storeSnapshot.SetPresent(slot, true);
                    storeSnapshot.SetValueFromBuffer(slot, buffer, offset + read);
                    read += componentSize;
                    present++;
                }

                if (present != componentCount)
                {
                    throw new InvalidOperationException("Savegame component count does not match the bitmap.");
                }
                storeSnapshot.SetLoadedCount(present);
                snapshot.Stores.Add(storeSnapshot);
            }

            world.Restore(snapshot);
            world.Events.Clear();
            world.Signals.Clear();
            world.Commands.Clear();
            return world.ComputeChecksum();
        }

        private static int BitmapBytes(int slotCount)
        {
            return (slotCount + 7) >> 3;
        }
    }
}
