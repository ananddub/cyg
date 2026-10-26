using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Small ring of recent world snapshots used by the desync overlay. It is a
    /// development/diagnostics helper, not part of the tick path.
    /// </summary>
    public sealed class CYGFrameSnapshotRing
    {
        private struct Entry
        {
            public int Tick;
            public CYGWorldSnapshot Snapshot;
        }

        private readonly List<Entry> _entries;
        private readonly int _capacity;

        public CYGFrameSnapshotRing(int capacity)
        {
            _capacity = Math.Max(2, capacity);
            _entries = new List<Entry>(_capacity);
        }

        public int Count
        {
            get { return _entries.Count; }
        }

        public int Capacity
        {
            get { return _capacity; }
        }

        public void Capture(int tick, CYGWorldSnapshot snapshot)
        {
            if (snapshot == null)
            {
                return;
            }

            for (int i = _entries.Count - 1; i >= 0; i--)
            {
                if (_entries[i].Tick == tick)
                {
                    _entries[i] = new Entry { Tick = tick, Snapshot = snapshot };
                    return;
                }
            }

            if (_entries.Count >= _capacity)
            {
                _entries.RemoveAt(0);
            }
            _entries.Add(new Entry { Tick = tick, Snapshot = snapshot });
        }

        public bool TryGet(int tick, out CYGWorldSnapshot snapshot)
        {
            for (int i = _entries.Count - 1; i >= 0; i--)
            {
                if (_entries[i].Tick == tick)
                {
                    snapshot = _entries[i].Snapshot;
                    return true;
                }
            }
            snapshot = null;
            return false;
        }

        public void Clear()
        {
            _entries.Clear();
        }
    }
}
