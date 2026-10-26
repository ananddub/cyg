using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Rollback-safe event buffer. Systems publish during a tick; dispatch is
    /// deferred for verified events until the tick is confirmed. A rollback
    /// removes every event from the rewound ticks, and stable keys suppress
    /// duplicate dispatch when those ticks are simulated again.
    /// </summary>
    public sealed class CYGEventBuffer
    {
        private sealed class Entry
        {
            public CYGEvent Event;
            public bool Dispatched;
            public bool Canceled;
        }

        private readonly List<Entry> _entries = new List<Entry>();
        private readonly HashSet<CYGEventKey> _dispatchedKeys = new HashSet<CYGEventKey>();
        private readonly int _retentionTicks;
        private int _currentTick = -1;
        private int _ordinal;
        private int _confirmedTick = -1;

        public CYGEventBuffer(int retentionTicks = 600)
        {
            if (retentionTicks < 1)
            {
                throw new ArgumentOutOfRangeException(nameof(retentionTicks));
            }
            _retentionTicks = retentionTicks;
        }

        public int CurrentTick
        {
            get { return _currentTick; }
        }

        public int ConfirmedTick
        {
            get { return _confirmedTick; }
        }

        public int PendingCount
        {
            get
            {
                int count = 0;
                for (int i = 0; i < _entries.Count; i++)
                {
                    if (!_entries[i].Dispatched && !_entries[i].Canceled)
                    {
                        count++;
                    }
                }
                return count;
            }
        }

        public void BeginTick(int tick, bool isResimulating = false)
        {
            if (tick < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(tick));
            }
            if (!isResimulating && tick < _currentTick)
            {
                throw new InvalidOperationException("Cannot move the event clock backwards without a rollback.");
            }

            _currentTick = tick;
            _ordinal = 0;
            Prune(tick);
        }

        public CYGEventKey Publish(CYGEvent evt)
        {
            if (evt == null)
            {
                throw new ArgumentNullException(nameof(evt));
            }
            if (_currentTick < 0)
            {
                throw new InvalidOperationException("BeginTick must be called before publishing events.");
            }

            CYGEventKey key = new CYGEventKey(_currentTick, evt.TypeId, _ordinal++);
            evt.Tick = _currentTick;
            evt.Key = key;
            _entries.Add(new Entry { Event = evt });
            return key;
        }

        /// <summary>Marks every buffered tick &lt;= <paramref name="tick"/> as verified.</summary>
        public void ConfirmUpTo(int tick)
        {
            if (tick > _confirmedTick)
            {
                _confirmedTick = tick;
            }
        }

        /// <summary>Marks predicted events at a tick as canceled so the view never sees them.</summary>
        public void CancelTick(int tick)
        {
            for (int i = 0; i < _entries.Count; i++)
            {
                Entry entry = _entries[i];
                if (entry.Event.Tick == tick)
                {
                    entry.Canceled = true;
                }
            }
        }

        /// <summary>Discards events fired at or after <paramref name="tick"/> during rollback.</summary>
        public void RollbackTo(int tick)
        {
            if (tick < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(tick));
            }

            for (int i = _entries.Count - 1; i >= 0; i--)
            {
                if (_entries[i].Event.Tick >= tick)
                {
                    _entries.RemoveAt(i);
                }
            }

            if (_confirmedTick >= tick)
            {
                _confirmedTick = tick - 1;
            }
            _currentTick = tick - 1;
            _ordinal = 0;
        }

        /// <summary>
        /// Appends dispatchable events in publish order. Verified events wait
        /// for their tick; local events dispatch immediately. Keys are consumed
        /// even after rollback, so a re-emitted event never reaches the view twice.
        /// </summary>
        public int DispatchReady(List<CYGEvent> output, bool forceAll = false)
        {
            if (output == null)
            {
                throw new ArgumentNullException(nameof(output));
            }

            int before = output.Count;
            for (int i = 0; i < _entries.Count; i++)
            {
                Entry entry = _entries[i];
                if (entry.Dispatched || entry.Canceled)
                {
                    continue;
                }
                if (!forceAll && entry.Event.VerifiedOnly && entry.Event.Tick > _confirmedTick)
                {
                    continue;
                }

                entry.Dispatched = true;
                if (_dispatchedKeys.Add(entry.Event.Key))
                {
                    output.Add(entry.Event);
                }
            }
            return output.Count - before;
        }

        public void Clear()
        {
            _entries.Clear();
            _dispatchedKeys.Clear();
            _currentTick = -1;
            _confirmedTick = -1;
            _ordinal = 0;
        }

        private void Prune(int tick)
        {
            int cutoff = tick - _retentionTicks;
            if (cutoff <= 0)
            {
                return;
            }

            int removed = 0;
            for (int i = _entries.Count - 1; i >= 0; i--)
            {
                if (_entries[i].Event.Tick < cutoff)
                {
                    _entries.RemoveAt(i);
                    removed++;
                }
            }
            if (removed > 0)
            {
                _dispatchedKeys.Clear();
            }
        }
    }
}
