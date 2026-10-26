using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Tick-local simulation signals. Signals are cleared by the runner at the
    /// start of every tick, never replicated, and never hashed. Systems poll
    /// them during the same tick; the view layer must not use them.
    /// </summary>
    public sealed class CYGSignalBus
    {
        public const int SignalCount = 32;

        private uint _mask;

        public uint Mask
        {
            get { return _mask; }
        }

        public bool Any
        {
            get { return _mask != 0u; }
        }

        public void Clear()
        {
            _mask = 0u;
        }

        public void Raise(int index)
        {
            RequireIndex(index);
            _mask |= 1u << index;
        }

        public void Set(int index, bool value)
        {
            RequireIndex(index);
            if (value)
            {
                _mask |= 1u << index;
            }
            else
            {
                _mask &= ~(1u << index);
            }
        }

        public bool IsSet(int index)
        {
            RequireIndex(index);
            return (_mask & (1u << index)) != 0u;
        }

        public void Raise(uint mask)
        {
            _mask |= mask;
        }

        public void Clear(uint mask)
        {
            _mask &= ~mask;
        }

        /// <summary>True when every bit in <paramref name="mask"/> is raised.</summary>
        public bool Has(uint mask)
        {
            return (_mask & mask) == mask;
        }

        /// <summary>True when at least one bit in <paramref name="mask"/> is raised.</summary>
        public bool HasAny(uint mask)
        {
            return (_mask & mask) != 0u;
        }

        private static void RequireIndex(int index)
        {
            if ((uint)index >= SignalCount)
            {
                throw new ArgumentOutOfRangeException(nameof(index), "Signal index must be in [0, 31].");
            }
        }
    }
}
