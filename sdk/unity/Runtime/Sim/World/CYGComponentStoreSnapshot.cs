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

        internal abstract void EnsureCapacity(int capacity);

        internal abstract void CaptureFrom(ICYGComponentStore store);

        internal abstract void RestoreTo(ICYGComponentStore store);

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

        public override ICYGComponentStore CreateStore()
        {
            return new CYGComponentStore<T>(_capacity, TypeId, SchemaHash);
        }
    }
}
