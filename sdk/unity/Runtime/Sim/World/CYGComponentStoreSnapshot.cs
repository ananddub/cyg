using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Typed snapshot buffer for one component store. Instances are created by
    /// <see cref="ICYGComponentStore.CreateSnapshot"/> and reused across saves;
    /// only the first capture (or a capacity increase) allocates.
    /// </summary>
    public abstract class CYGComponentStoreSnapshot
    {
        protected CYGComponentStoreSnapshot(int typeId, uint schemaHash)
        {
            TypeId = typeId;
            SchemaHash = schemaHash;
        }

        public int TypeId { get; }

        public uint SchemaHash { get; }

        public abstract int Count { get; }

        public abstract int Capacity { get; }

        /// <summary>Wire size in bytes of one component value.</summary>
        internal abstract int ComponentSize { get; }

        internal abstract void EnsureCapacity(int capacity);

        /// <summary>Writes a present slot's value using the generated wire codec.</summary>
        internal abstract void CopyComponentToBuffer(int index, byte[] buffer, int offset);

        /// <summary>True when the snapshot slot holds a component value.</summary>
        internal abstract bool IsPresent(int index);

        internal abstract void CaptureFrom(ICYGComponentStore store);

        internal abstract void RestoreTo(ICYGComponentStore store);

        internal abstract void PrepareForLoad(int capacity);

        internal abstract void SetPresent(int index, bool present);

        internal abstract void SetValueFromBuffer(int index, byte[] buffer, int offset);

        internal abstract void SetLoadedCount(int count);

        /// <summary>Hashes presence and values in entity slot order.</summary>
        public abstract uint ComputeChecksum();

        /// <summary>
        /// Returns the first entity slot whose presence or value differs, or -1.
        /// Used by desync diagnostics after a checksum mismatch.
        /// </summary>
        public abstract int FindFirstDifference(CYGComponentStoreSnapshot other);

        /// <summary>Creates an empty store matching this snapshot's type and capacity.</summary>
        public abstract ICYGComponentStore CreateStore();
    }

    /// <summary>Snapshot storage for <see cref="CYGComponentStore{T}"/>.</summary>
    public sealed class CYGComponentStoreSnapshot<T> : CYGComponentStoreSnapshot
        where T : struct, ICYGComponent
    {
        private T[] _values;
        private bool[] _present;
        private int _capacity;
        private int _count;

        public CYGComponentStoreSnapshot(int capacity)
            : this(capacity, default(T).ComponentTypeId, default(T).ComponentSchemaHash)
        {
        }

        public CYGComponentStoreSnapshot(int capacity, int typeId, uint schemaHash)
            : base(typeId, schemaHash)
        {
            if (capacity < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(capacity));
            }
            if (default(T).ComponentTypeId != typeId)
            {
                throw new ArgumentException(
                    $"TypeId {typeId} does not match {typeof(T).Name}.ComponentTypeId {default(T).ComponentTypeId}.",
                    nameof(typeId));
            }
            if (default(T).ComponentSchemaHash != schemaHash)
            {
                throw new ArgumentException(
                    $"SchemaHash 0x{schemaHash:X8} does not match {typeof(T).Name}.ComponentSchemaHash 0x{default(T).ComponentSchemaHash:X8}.",
                    nameof(schemaHash));
            }

            _capacity = capacity;
            _values = capacity == 0 ? Array.Empty<T>() : new T[capacity];
            _present = capacity == 0 ? Array.Empty<bool>() : new bool[capacity];
        }

        public override int Count
        {
            get { return _count; }
        }

        public override int Capacity
        {
            get { return _capacity; }
        }

        internal override int ComponentSize
        {
            get { return default(T).WireSize; }
        }

        internal override void CopyComponentToBuffer(int index, byte[] buffer, int offset)
        {
            if (buffer == null)
            {
                throw new ArgumentNullException(nameof(buffer));
            }
            if ((uint)index >= (uint)_capacity || !_present[index])
            {
                throw new InvalidOperationException(
                    "Snapshot slot " + index + " does not have component type " + TypeId + ".");
            }

            _values[index].WriteTo(buffer, offset);
        }

        internal override bool IsPresent(int index)
        {
            return (uint)index < (uint)_capacity && _present[index];
        }

        internal override void EnsureCapacity(int capacity)
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

        internal override void CaptureFrom(ICYGComponentStore store)
        {
            if (store == null)
            {
                throw new ArgumentNullException(nameof(store));
            }
            if (store.TypeId != TypeId || store.SchemaHash != SchemaHash)
            {
                throw new InvalidOperationException(
                    $"Snapshot {TypeId}/0x{SchemaHash:X8} cannot capture store {store.TypeId}/0x{store.SchemaHash:X8}.");
            }

            CYGComponentStore<T> typed = store as CYGComponentStore<T>;
            if (typed == null)
            {
                throw new InvalidOperationException(
                    $"Component store {store.TypeId} is not a {typeof(CYGComponentStore<T>).Name}.");
            }

            EnsureCapacity(typed.Capacity);
            Array.Clear(_values, 0, _values.Length);
            Array.Clear(_present, 0, _present.Length);
            typed.CopyToSnapshot(_values, _present);
            _count = typed.Count;
        }

        internal override void RestoreTo(ICYGComponentStore store)
        {
            if (store == null)
            {
                throw new ArgumentNullException(nameof(store));
            }

            CYGComponentStore<T> typed = store as CYGComponentStore<T>;
            if (typed == null)
            {
                throw new InvalidOperationException(
                    $"Component store {store.TypeId} is not a {typeof(CYGComponentStore<T>).Name}.");
            }
            if (typed.TypeId != TypeId || typed.SchemaHash != SchemaHash)
            {
                throw new InvalidOperationException(
                    $"Snapshot {TypeId}/0x{SchemaHash:X8} cannot restore store {typed.TypeId}/0x{typed.SchemaHash:X8}.");
            }

            typed.RestoreFromSnapshot(_values, _present, _count, _capacity);
        }

        internal override void PrepareForLoad(int capacity)
        {
            EnsureCapacity(capacity);
            Array.Clear(_values, 0, _values.Length);
            Array.Clear(_present, 0, _present.Length);
            _count = 0;
        }

        internal override void SetPresent(int index, bool present)
        {
            EnsureCapacity(index + 1);
            _present[index] = present;
        }

        internal override void SetValueFromBuffer(int index, byte[] buffer, int offset)
        {
            EnsureCapacity(index + 1);
            _values[index].ReadFrom(buffer, offset);
        }

        internal override void SetLoadedCount(int count)
        {
            _count = count;
        }

        public override uint ComputeChecksum()
        {
            uint crc = 0xFFFFFFFFu;
            for (int i = 0; i < _capacity; i++)
            {
                if (!_present[i])
                {
                    continue;
                }

                CYGChecksum.HashInt(ref crc, i);
                _values[i].ComputeChecksum(ref crc);
            }
            return crc ^ 0xFFFFFFFFu;
        }

        public override int FindFirstDifference(CYGComponentStoreSnapshot other)
        {
            if (other == null)
            {
                throw new ArgumentNullException(nameof(other));
            }

            CYGComponentStoreSnapshot<T> typed = other as CYGComponentStoreSnapshot<T>;
            if (typed == null)
            {
                throw new InvalidOperationException(
                    $"Snapshot store {other.TypeId}/0x{other.SchemaHash:X8} is not a {typeof(CYGComponentStoreSnapshot<T>).Name}.");
            }

            int length = Math.Max(_capacity, typed._capacity);
            for (int i = 0; i < length; i++)
            {
                bool leftPresent = (uint)i < (uint)_capacity && _present[i];
                bool rightPresent = (uint)i < (uint)typed._capacity && typed._present[i];
                if (leftPresent != rightPresent)
                {
                    return i;
                }
                if (leftPresent && ValueChecksum(_values[i]) != ValueChecksum(typed._values[i]))
                {
                    return i;
                }
            }

            return -1;
        }

        private static uint ValueChecksum(T value)
        {
            uint crc = 0xFFFFFFFFu;
            value.ComputeChecksum(ref crc);
            return crc ^ 0xFFFFFFFFu;
        }

        public override ICYGComponentStore CreateStore()
        {
            return new CYGComponentStore<T>(_capacity, TypeId, SchemaHash);
        }
    }
}
