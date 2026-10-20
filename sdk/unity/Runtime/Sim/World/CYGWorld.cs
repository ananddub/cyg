using System;
using System.Collections.Generic;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Deterministic entity/component world. Entity references carry a slot
    /// generation so destroyed slots can be recycled without making stale
    /// references valid again. Component stores are registered by type id and
    /// hashed in id order for cross-client desync detection.
    /// </summary>
    public sealed class CYGWorld
    {
        private readonly List<ICYGComponentStore> _stores = new List<ICYGComponentStore>();
        private readonly Dictionary<int, int> _storeIndexByTypeId = new Dictionary<int, int>();
        private ICYGComponentStore[] _sortedStores = Array.Empty<ICYGComponentStore>();
        private bool _sortDirty;

        private int[] _versions = Array.Empty<int>();
        private bool[] _alive = Array.Empty<bool>();
        private readonly Stack<int> _free = new Stack<int>();
        private int _capacity;
        private int _nextIndex;
        private int _count;
        private uint _structuralVersion;

        public CYGWorld()
        {
        }

        public int Capacity
        {
            get { return _capacity; }
        }

        public int EntityCount
        {
            get { return _count; }
        }

        /// <summary>Increases whenever an entity or component membership changes.</summary>
        public uint StructuralVersion
        {
            get { return _structuralVersion; }
        }

        public IReadOnlyList<ICYGComponentStore> Stores
        {
            get { return _stores; }
        }

        public void Register<T>()
            where T : struct, ICYGComponent
        {
            Register(new CYGComponentStore<T>(_nextIndex));
        }

        public void Register(ICYGComponentStore store)
        {
            if (store == null)
            {
                throw new ArgumentNullException(nameof(store));
            }
            if (_storeIndexByTypeId.ContainsKey(store.TypeId))
            {
                throw new InvalidOperationException(
                    "A component store with TypeId " + store.TypeId + " is already registered.");
            }

            if (store.Capacity < _nextIndex)
            {
                store.EnsureCapacity(_nextIndex);
            }

            _storeIndexByTypeId.Add(store.TypeId, _stores.Count);
            _stores.Add(store);
            _sortDirty = true;
        }

        public bool TryGetStore(int typeId, out ICYGComponentStore store)
        {
            int index;
            if (_storeIndexByTypeId.TryGetValue(typeId, out index))
            {
                store = _stores[index];
                return true;
            }

            store = null;
            return false;
        }

        public CYGEntityRef CreateEntity()
        {
            int index;
            if (_free.Count > 0)
            {
                index = _free.Pop();
            }
            else
            {
                index = _nextIndex;
                EnsureSlotCapacity(index + 1);
                _nextIndex++;
            }

            if (_versions[index] == 0)
            {
                _versions[index] = 1;
            }

            _alive[index] = true;
            _count++;
            _structuralVersion++;
            return new CYGEntityRef(index, _versions[index]);
        }

        /// <summary>
        /// Destroys an entity, removes all its components, and bumps the slot
        /// generation so the reference can never resolve again.
        /// </summary>
        public bool DestroyEntity(CYGEntityRef entity)
        {
            if (!Exists(entity))
            {
                return false;
            }

            int index = entity.Index;
            for (int i = 0; i < _stores.Count; i++)
            {
                _stores[i].Remove(index);
            }

            int next = _versions[index] + 1;
            if (next == 0)
            {
                next = 1;
            }
            _versions[index] = next;
            _alive[index] = false;
            _free.Push(index);
            _count--;
            _structuralVersion++;
            return true;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool Exists(CYGEntityRef entity)
        {
            return (uint)entity.Index < (uint)_nextIndex
                && _alive[entity.Index]
                && _versions[entity.Index] == entity.Version;
        }

        public void Set<T>(CYGEntityRef entity, in T value)
            where T : struct, ICYGComponent
        {
            RequireAlive(entity);
            int typeId = default(T).ComponentTypeId;
            int storeIndex;
            if (!_storeIndexByTypeId.TryGetValue(typeId, out storeIndex))
            {
                throw new InvalidOperationException(
                    "Component " + typeof(T).Name + " (TypeId " + typeId + ") is not registered with this world.");
            }

            CYGComponentStore<T> store = _stores[storeIndex] as CYGComponentStore<T>;
            if (store == null)
            {
                throw new InvalidOperationException(
                    "Registered store for TypeId " + typeId + " is not a " + typeof(CYGComponentStore<T>).Name + ".");
            }

            bool added = !store.Has(entity.Index);
            store.EnsureCapacity(entity.Index + 1);
            store.Set(entity.Index, value);
            if (added)
            {
                _structuralVersion++;
            }
        }

        public bool TryGet<T>(CYGEntityRef entity, out T value)
            where T : struct, ICYGComponent
        {
            value = default(T);
            if (!Exists(entity))
            {
                return false;
            }

            int typeId = default(T).ComponentTypeId;
            int storeIndex;
            if (!_storeIndexByTypeId.TryGetValue(typeId, out storeIndex))
            {
                return false;
            }

            CYGComponentStore<T> store = _stores[storeIndex] as CYGComponentStore<T>;
            return store != null && store.TryGet(entity.Index, out value);
        }

        public bool Has<T>(CYGEntityRef entity)
            where T : struct, ICYGComponent
        {
            if (!Exists(entity))
            {
                return false;
            }

            int typeId = default(T).ComponentTypeId;
            int storeIndex;
            if (!_storeIndexByTypeId.TryGetValue(typeId, out storeIndex))
            {
                return false;
            }

            return _stores[storeIndex].Has(entity.Index);
        }

        public bool Remove<T>(CYGEntityRef entity)
            where T : struct, ICYGComponent
        {
            if (!Exists(entity))
            {
                return false;
            }

            int typeId = default(T).ComponentTypeId;
            int storeIndex;
            if (!_storeIndexByTypeId.TryGetValue(typeId, out storeIndex))
            {
                return false;
            }

            bool removed = _stores[storeIndex].Remove(entity.Index);
            if (removed)
            {
                _structuralVersion++;
            }
            return removed;
        }

        public void Clear()
        {
            for (int i = 0; i < _stores.Count; i++)
            {
                _stores[i].Clear();
            }

            _free.Clear();
            for (int i = 0; i < _nextIndex; i++)
            {
                if (_alive[i])
                {
                    int next = _versions[i] + 1;
                    if (next == 0)
                    {
                        next = 1;
                    }
                    _versions[i] = next;
                    _alive[i] = false;
                }
                _free.Push(i);
            }

            _count = 0;
            _structuralVersion++;
        }

        /// <summary>
        /// Hashes entity slots and every registered store. Stores are hashed in
        /// type-id order so registration order cannot change the result.
        /// </summary>
        public uint ComputeChecksum()
        {
            uint crc = 0xFFFFFFFFu;
            ComputeChecksum(ref crc);
            return crc;
        }

        public void ComputeChecksum(ref uint crc)
        {
            CYGChecksum.HashInt(ref crc, _count);
            CYGChecksum.HashUInt(ref crc, _structuralVersion);

            for (int i = 0; i < _nextIndex; i++)
            {
                if (!_alive[i])
                {
                    continue;
                }

                CYGChecksum.HashInt(ref crc, i);
                CYGChecksum.HashInt(ref crc, _versions[i]);
            }

            if (_sortDirty)
            {
                _sortedStores = _stores.ToArray();
                Array.Sort(_sortedStores, CompareStores);
                _sortDirty = false;
            }

            CYGChecksum.HashInt(ref crc, _sortedStores.Length);
            for (int i = 0; i < _sortedStores.Length; i++)
            {
                _sortedStores[i].ComputeChecksum(ref crc);
            }
        }

        public CYGWorldSnapshot CreateSnapshot()
        {
            CYGWorldSnapshot snapshot = new CYGWorldSnapshot();
            Capture(snapshot);
            return snapshot;
        }

        /// <summary>
        /// Captures the current world into an existing snapshot. Store buffers
        /// and slot arrays are reused, so repeated captures do not allocate
        /// once the snapshot has grown to its working size.
        /// </summary>
        public void Capture(CYGWorldSnapshot snapshot)
        {
            if (snapshot == null)
            {
                throw new ArgumentNullException(nameof(snapshot));
            }

            snapshot.EnsureSlotCapacity(_nextIndex);
            Array.Copy(_versions, snapshot.Versions, _nextIndex);
            Array.Copy(_alive, snapshot.Alive, _nextIndex);
            snapshot.NextIndex = _nextIndex;
            snapshot.Count = _count;
            snapshot.Capacity = _capacity;
            snapshot.StructuralVersion = _structuralVersion;

            int[] free = _free.ToArray();
            snapshot.EnsureFreeCapacity(free.Length);
            Array.Copy(free, snapshot.Free, free.Length);
            snapshot.FreeCount = free.Length;

            snapshot.SyncStores(_stores);
            for (int i = 0; i < _stores.Count; i++)
            {
                snapshot.Stores[i].CaptureFrom(_stores[i]);
            }
            snapshot.TrimStores(_stores.Count);
        }

        /// <summary>
        /// Restores a snapshot captured from this world. Registration order and
        /// component types must still match; the snapshot cannot silently
        /// restore into a different schema.
        /// </summary>
        public void Restore(CYGWorldSnapshot snapshot)
        {
            if (snapshot == null)
            {
                throw new ArgumentNullException(nameof(snapshot));
            }
            if (snapshot.Stores.Count != _stores.Count)
            {
                throw new InvalidOperationException(
                    "Snapshot has " + snapshot.Stores.Count + " component stores but the world has " + _stores.Count + ".");
            }
            for (int i = 0; i < _stores.Count; i++)
            {
                CYGComponentStoreSnapshot storeSnapshot = snapshot.Stores[i];
                if (storeSnapshot.TypeId != _stores[i].TypeId || storeSnapshot.SchemaHash != _stores[i].SchemaHash)
                {
                    throw new InvalidOperationException(
                        "Snapshot store " + storeSnapshot.TypeId + "/0x" + storeSnapshot.SchemaHash.ToString("X8")
                        + " does not match registered store " + _stores[i].TypeId + "/0x" + _stores[i].SchemaHash.ToString("X8") + ".");
                }
            }

            EnsureSlotCapacity(snapshot.NextIndex);
            Array.Copy(snapshot.Versions, _versions, snapshot.NextIndex);
            Array.Copy(snapshot.Alive, _alive, snapshot.NextIndex);
            for (int i = snapshot.NextIndex; i < _capacity; i++)
            {
                _versions[i] = 0;
                _alive[i] = false;
            }

            _nextIndex = snapshot.NextIndex;
            _count = snapshot.Count;
            _structuralVersion = snapshot.StructuralVersion;

            _free.Clear();
            for (int i = snapshot.FreeCount - 1; i >= 0; i--)
            {
                _free.Push(snapshot.Free[i]);
            }

            for (int i = 0; i < _stores.Count; i++)
            {
                snapshot.Stores[i].RestoreTo(_stores[i]);
            }
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private void RequireAlive(CYGEntityRef entity)
        {
            if (!Exists(entity))
            {
                throw new InvalidOperationException("Entity " + entity + " is not alive in this world.");
            }
        }

        private void EnsureSlotCapacity(int capacity)
        {
            if (capacity <= _capacity)
            {
                return;
            }

            int target = _capacity < 4 ? 4 : _capacity;
            while (target < capacity)
            {
                if (target > int.MaxValue / 2)
                {
                    target = capacity;
                    break;
                }
                target *= 2;
            }

            Array.Resize(ref _versions, target);
            Array.Resize(ref _alive, target);
            _capacity = target;
        }

        private static int CompareStores(ICYGComponentStore left, ICYGComponentStore right)
        {
            return left.TypeId.CompareTo(right.TypeId);
        }
    }
}
