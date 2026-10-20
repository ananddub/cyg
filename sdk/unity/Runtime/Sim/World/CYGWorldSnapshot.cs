using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Reusable snapshot of an entire world: entity slots, the free list, and
    /// every registered component store. Capture and restore are allocation
    /// free once the snapshot has grown to the world's working size.
    /// </summary>
    public sealed class CYGWorldSnapshot
    {
        internal int Capacity;
        internal int NextIndex;
        internal int Count;
        internal int[] Versions = Array.Empty<int>();
        internal bool[] Alive = Array.Empty<bool>();
        internal int[] Free = Array.Empty<int>();
        internal int FreeCount;
        internal uint StructuralVersion;

        internal readonly List<CYGComponentStoreSnapshot> Stores = new List<CYGComponentStoreSnapshot>();

        public int EntityCount
        {
            get { return Count; }
        }

        public int StoreCount
        {
            get { return Stores.Count; }
        }

        internal void EnsureSlotCapacity(int capacity)
        {
            if (capacity <= Versions.Length && capacity <= Alive.Length)
            {
                return;
            }

            int target = Versions.Length < 4 ? 4 : Versions.Length;
            while (target < capacity)
            {
                if (target > int.MaxValue / 2)
                {
                    target = capacity;
                    break;
                }
                target *= 2;
            }

            Array.Resize(ref Versions, target);
            Array.Resize(ref Alive, target);
        }

        internal void EnsureFreeCapacity(int capacity)
        {
            if (capacity <= Free.Length)
            {
                return;
            }

            int target = Free.Length < 4 ? 4 : Free.Length;
            while (target < capacity)
            {
                if (target > int.MaxValue / 2)
                {
                    target = capacity;
                    break;
                }
                target *= 2;
            }

            Array.Resize(ref Free, target);
        }

        internal void SyncStores(IReadOnlyList<ICYGComponentStore> stores)
        {
            while (Stores.Count < stores.Count)
            {
                Stores.Add(null);
            }

            for (int i = 0; i < stores.Count; i++)
            {
                CYGComponentStoreSnapshot existing = Stores[i];
                if (existing == null
                    || existing.TypeId != stores[i].TypeId
                    || existing.SchemaHash != stores[i].SchemaHash)
                {
                    Stores[i] = stores[i].CreateSnapshot(stores[i].Capacity);
                }
            }
        }

        internal void TrimStores(int count)
        {
            if (Stores.Count <= count)
            {
                return;
            }

            Stores.RemoveRange(count, Stores.Count - count);
        }
    }
}
