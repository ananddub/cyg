using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Non-generic view over a component store. Worlds keep stores in this form
    /// so checksum and snapshot walks do not need reflection.
    /// </summary>
    public interface ICYGComponentStore
    {
        /// <summary>Stable component id (matches <see cref="ICYGComponent.ComponentTypeId"/>).</summary>
        int TypeId { get; }

        /// <summary>Field-layout hash of the component type.</summary>
        uint SchemaHash { get; }

        /// <summary>Number of entity slots that currently have this component.</summary>
        int Count { get; }

        /// <summary>Number of entity slots the store can hold without growing.</summary>
        int Capacity { get; }

        /// <summary>Grows the backing arrays so at least <paramref name="capacity"/> slots fit.</summary>
        void EnsureCapacity(int capacity);

        /// <summary>True when the entity slot currently has this component.</summary>
        bool Has(int entityIndex);

        /// <summary>Removes the component from an entity slot. Returns false if it was absent.</summary>
        bool Remove(int entityIndex);

        /// <summary>Replaces this store's contents with a copy of <paramref name="source"/>.</summary>
        void CopyFrom(ICYGComponentStore source);

        /// <summary>Removes every component from the store while keeping its type registration.</summary>
        void Clear();

        /// <summary>Hashes the store into <paramref name="crc"/> in entity index order.</summary>
        void ComputeChecksum(ref uint crc);

        /// <summary>Allocates a snapshot shell for this store type. Arrays are filled by <c>CaptureFrom</c>.</summary>
        CYGComponentStoreSnapshot CreateSnapshot(int capacity);
    }

    /// <summary>
    /// Dense component storage keyed by entity slot index.
    /// Operations never box <typeparamref name="T"/> and tick-path calls do not
    /// allocate once the store has been created with its final capacity.
    /// </summary>
    public sealed class CYGComponentStore<T> : ICYGComponentStore
        where T : struct, ICYGComponent
    {
        private T[] _values;
        private bool[] _present;
        private int _capacity;
        private int _count;
        private readonly int _typeId;
        private readonly uint _schemaHash;

        public CYGComponentStore()
            : this(0)
        {
        }

        public CYGComponentStore(int capacity)
            : this(capacity, default(T).ComponentTypeId, default(T).ComponentSchemaHash)
        {
        }

        public CYGComponentStore(int capacity, int typeId, uint schemaHash)
        {
            if (capacity < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(capacity));
            }

            // Constrained calls on default(T) stay unboxed. This also fails fast
            // when a hand-written store is paired with the wrong component type.
            int declaredTypeId = default(T).ComponentTypeId;
            uint declaredSchema = default(T).ComponentSchemaHash;
            if (declaredTypeId != typeId)
            {
                throw new ArgumentException(
                    $"TypeId {typeId} does not match {typeof(T).Name}.ComponentTypeId {declaredTypeId}.",
                    nameof(typeId));
            }
            if (declaredSchema != schemaHash)
            {
                throw new ArgumentException(
                    $"SchemaHash 0x{schemaHash:X8} does not match {typeof(T).Name}.ComponentSchemaHash 0x{declaredSchema:X8}.",
                    nameof(schemaHash));
            }

            _typeId = typeId;
            _schemaHash = schemaHash;
            _capacity = capacity;
            _values = capacity == 0 ? Array.Empty<T>() : new T[capacity];
            _present = capacity == 0 ? Array.Empty<bool>() : new bool[capacity];
        }

        public int TypeId
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return _typeId; }
        }

        public uint SchemaHash
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return _schemaHash; }
        }

        public int Count
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return _count; }
        }

        public int Capacity
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return _capacity; }
        }

        public void EnsureCapacity(int capacity)
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

            Array.Resize(ref _values, target);
            Array.Resize(ref _present, target);
            _capacity = target;
        }

        /// <summary>Adds or replaces the component on an entity slot.</summary>
        public void Set(int entityIndex, in T value)
        {
            if ((uint)entityIndex >= (uint)_capacity)
            {
                throw new ArgumentOutOfRangeException(nameof(entityIndex));
            }

            if (!_present[entityIndex])
            {
                _present[entityIndex] = true;
                _count++;
            }
            _values[entityIndex] = value;
        }

        public bool TryGet(int entityIndex, out T value)
        {
            if ((uint)entityIndex < (uint)_capacity && _present[entityIndex])
            {
                value = _values[entityIndex];
                return true;
            }

            value = default;
            return false;
        }

        /// <summary>Removes the component from an entity slot. Returns false if it was absent.</summary>
        public bool Remove(int entityIndex)
        {
            if ((uint)entityIndex >= (uint)_capacity || !_present[entityIndex])
            {
                return false;
            }

            _present[entityIndex] = false;
            _values[entityIndex] = default;
            _count--;
            return true;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool Has(int entityIndex)
        {
            return (uint)entityIndex < (uint)_capacity && _present[entityIndex];
        }

        public void Clear()
        {
            Array.Clear(_values, 0, _values.Length);
            Array.Clear(_present, 0, _present.Length);
            _count = 0;
        }

        public void CopyFrom(ICYGComponentStore source)
        {
            if (source == null)
            {
                throw new ArgumentNullException(nameof(source));
            }
            if (ReferenceEquals(source, this))
            {
                return;
            }
            if (source.TypeId != _typeId || source.SchemaHash != _schemaHash)
            {
                throw new InvalidOperationException(
                    $"Cannot copy component store {source.TypeId}/0x{source.SchemaHash:X8} into {_typeId}/0x{_schemaHash:X8}.");
            }

            CYGComponentStore<T> typed = source as CYGComponentStore<T>;
            if (typed == null)
            {
                throw new InvalidOperationException(
                    $"Component store {source.TypeId} is not a {typeof(CYGComponentStore<T>).Name}.");
            }

            EnsureCapacity(typed._capacity);
            Clear();
            Array.Copy(typed._values, _values, typed._capacity);
            Array.Copy(typed._present, _present, typed._capacity);
            _count = typed._count;
        }

        /// <summary>
        /// Hashes store identity, count, and each present component in entity
        /// index order. The entity index is mixed in so swapping two entities'
        /// values changes the checksum.
        /// </summary>
        public void ComputeChecksum(ref uint crc)
        {
            CYGChecksum.HashInt(ref crc, _typeId);
            CYGChecksum.HashUInt(ref crc, _schemaHash);
            CYGChecksum.HashInt(ref crc, _count);

            for (int i = 0; i < _capacity; i++)
            {
                if (!_present[i])
                {
                    continue;
                }

                CYGChecksum.HashInt(ref crc, i);
                _values[i].ComputeChecksum(ref crc);
            }
        }

        public CYGComponentStoreSnapshot CreateSnapshot(int capacity)
        {
            if (capacity < _capacity)
            {
                capacity = _capacity;
            }
            return new CYGComponentStoreSnapshot<T>(capacity, _typeId, _schemaHash);
        }

        internal void CopyToSnapshot(T[] values, bool[] present)
        {
            Array.Copy(_values, values, _capacity);
            Array.Copy(_present, present, _capacity);
        }

        internal void RestoreFromSnapshot(T[] values, bool[] present, int count, int sourceCapacity)
        {
            EnsureCapacity(sourceCapacity);
            Array.Clear(_values, 0, _values.Length);
            Array.Clear(_present, 0, _present.Length);

            int copy = sourceCapacity;
            if (copy > values.Length)
            {
                copy = values.Length;
            }
            if (copy > present.Length)
            {
                copy = present.Length;
            }

            Array.Copy(values, _values, copy);
            Array.Copy(present, _present, copy);
            _count = count;
        }
    }
}
