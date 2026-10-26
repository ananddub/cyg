using System;
using System.Collections;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame
{
    public enum CYGSyncListOp : byte
    {
        Add = 0,
        Insert = 1,
        Set = 2,
        RemoveAt = 3,
        Clear = 4,
    }

    /// <summary>
    /// Synchronized collection that replicates item additions, removals, and changes
    /// across the network with minimal delta payloads.
    /// </summary>
    public class CYGSyncList<T> : IList<T>, IReadOnlyList<T>, ICYGSyncVar
    {
        struct PendingChange
        {
            public CYGSyncListOp Op;
            public int Index;
            public T Item;
        }

        readonly List<T> _items = new List<T>();
        readonly List<PendingChange> _changes = new List<PendingChange>();

        /// <inheritdoc/>
        public byte Index { get; set; }

        /// <inheritdoc/>
        public bool IsDirty
        {
            get { return _changes.Count > 0; }
        }

        /// <inheritdoc/>
        public void SetDirty()
        {
            for (int i = 0; i < _items.Count; i++)
            {
                _changes.Add(new PendingChange { Op = CYGSyncListOp.Set, Index = i, Item = _items[i] });
            }
        }

        /// <summary>When true, local owner may mutate the list; when false, only server writes.</summary>
        public bool OwnerAuth { get; set; }

        public event Action<int, T> OnItemAdded;
        public event Action<int, T> OnItemRemoved;
        public event Action<int, T, T> OnItemUpdated;
        public event Action OnCleared;

        public int Count => _items.Count;
        public bool IsReadOnly => false;

        public T this[int index]
        {
            get => _items[index];
            set
            {
                T old = _items[index];
                _items[index] = value;
                _changes.Add(new PendingChange { Op = CYGSyncListOp.Set, Index = index, Item = value });
                OnItemUpdated?.Invoke(index, old, value);
            }
        }

        public void Add(T item)
        {
            int index = _items.Count;
            _items.Add(item);
            _changes.Add(new PendingChange { Op = CYGSyncListOp.Add, Index = index, Item = item });
            OnItemAdded?.Invoke(index, item);
        }

        public void Insert(int index, T item)
        {
            _items.Insert(index, item);
            _changes.Add(new PendingChange { Op = CYGSyncListOp.Insert, Index = index, Item = item });
            OnItemAdded?.Invoke(index, item);
        }

        public bool Remove(T item)
        {
            int index = _items.IndexOf(item);
            if (index < 0) return false;
            RemoveAt(index);
            return true;
        }

        public void RemoveAt(int index)
        {
            T old = _items[index];
            _items.RemoveAt(index);
            _changes.Add(new PendingChange { Op = CYGSyncListOp.RemoveAt, Index = index, Item = default });
            OnItemRemoved?.Invoke(index, old);
        }

        public void Clear()
        {
            _items.Clear();
            _changes.Add(new PendingChange { Op = CYGSyncListOp.Clear, Index = 0, Item = default });
            OnCleared?.Invoke();
        }

        public bool Contains(T item) => _items.Contains(item);
        public void CopyTo(T[] array, int arrayIndex) => _items.CopyTo(array, arrayIndex);
        public int IndexOf(T item) => _items.IndexOf(item);
        public IEnumerator<T> GetEnumerator() => _items.GetEnumerator();
        IEnumerator IEnumerable.GetEnumerator() => _items.GetEnumerator();

        /// <inheritdoc/>
        public void PackDirty(CYGBitWriter writer)
        {
            var packer = CYGSyncVar.GetPacker(typeof(T));
            writer.WriteByte((byte)_changes.Count);
            for (int i = 0; i < _changes.Count; i++)
            {
                PendingChange change = _changes[i];
                writer.WriteByte((byte)change.Op);
                switch (change.Op)
                {
                    case CYGSyncListOp.Add:
                    case CYGSyncListOp.Insert:
                        writer.WriteInt32(change.Index);
                        packer.Pack(writer, change.Item);
                        break;
                    case CYGSyncListOp.Set:
                        writer.WriteInt32(change.Index);
                        packer.Pack(writer, change.Item);
                        break;
                    case CYGSyncListOp.RemoveAt:
                        writer.WriteInt32(change.Index);
                        break;
                    case CYGSyncListOp.Clear:
                        break;
                }
            }
            _changes.Clear();
        }

        /// <inheritdoc/>
        public void ApplyRemoteAndClear(CYGBitReader reader)
        {
            var packer = CYGSyncVar.GetPacker(typeof(T));
            byte count = reader.ReadByte();
            for (int i = 0; i < count; i++)
            {
                CYGSyncListOp op = (CYGSyncListOp)reader.ReadByte();
                switch (op)
                {
                    case CYGSyncListOp.Add:
                    case CYGSyncListOp.Insert:
                    {
                        int idx = reader.ReadInt32();
                        T item = (T)packer.Unpack(reader);
                        if (idx >= 0 && idx <= _items.Count)
                            _items.Insert(idx, item);
                        else
                            _items.Add(item);
                        OnItemAdded?.Invoke(idx, item);
                        break;
                    }
                    case CYGSyncListOp.Set:
                    {
                        int idx = reader.ReadInt32();
                        T item = (T)packer.Unpack(reader);
                        if (idx >= 0 && idx < _items.Count)
                        {
                            T old = _items[idx];
                            _items[idx] = item;
                            OnItemUpdated?.Invoke(idx, old, item);
                        }
                        break;
                    }
                    case CYGSyncListOp.RemoveAt:
                    {
                        int idx = reader.ReadInt32();
                        if (idx >= 0 && idx < _items.Count)
                        {
                            T old = _items[idx];
                            _items.RemoveAt(idx);
                            OnItemRemoved?.Invoke(idx, old);
                        }
                        break;
                    }
                    case CYGSyncListOp.Clear:
                    {
                        _items.Clear();
                        OnCleared?.Invoke();
                        break;
                    }
                }
            }
            _changes.Clear();
        }
    }
}
