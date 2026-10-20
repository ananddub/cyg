using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Advances a <see cref="CYGWorld"/> one tick at a time and keeps a ring of
    /// recent snapshots so the simulation can be rolled back and re-simulated
    /// with corrected input.
    /// </summary>
    public sealed class CYGWorldRunner
    {
        private sealed class FrameSnapshot
        {
            public int Frame;
            public bool Valid;
            public CYGWorldSnapshot State;
        }

        private readonly CYGWorld _world;
        private readonly FrameSnapshot[] _ring;
        private int _next;
        private int _frame;

        public CYGWorldRunner(CYGWorld world, int rollbackCapacity = 64)
        {
            if (world == null)
            {
                throw new ArgumentNullException(nameof(world));
            }
            if (rollbackCapacity < 1)
            {
                throw new ArgumentOutOfRangeException(nameof(rollbackCapacity));
            }

            _world = world;
            _ring = new FrameSnapshot[rollbackCapacity];
            for (int i = 0; i < _ring.Length; i++)
            {
                _ring[i] = new FrameSnapshot();
            }
        }

        public CYGWorld World
        {
            get { return _world; }
        }

        /// <summary>The frame number of the most recently completed tick.</summary>
        public int Frame
        {
            get { return _frame; }
        }

        public int RollbackCapacity
        {
            get { return _ring.Length; }
        }

        public uint Checksum
        {
            get { return _world.ComputeChecksum(); }
        }

        /// <summary>
        /// Runs one simulation step and records the resulting state. The
        /// snapshot is taken after the callback returns, so rolling back N
        /// frames restores the world exactly as it was at the end of that tick.
        /// </summary>
        public void Tick(Action<CYGWorld> simulate)
        {
            if (simulate == null)
            {
                throw new ArgumentNullException(nameof(simulate));
            }

            simulate(_world);
            _frame++;

            FrameSnapshot slot = _ring[_next];
            if (slot.State == null)
            {
                slot.State = _world.CreateSnapshot();
            }
            else
            {
                _world.Capture(slot.State);
            }
            slot.Frame = _frame;
            slot.Valid = true;
            _next = (_next + 1) % _ring.Length;
        }

        /// <summary>
        /// Restores the world to the end of <paramref name="framesBack"/> ticks
        /// ago. Returns false when that frame is no longer in the ring.
        /// </summary>
        public bool Rollback(int framesBack)
        {
            if (framesBack < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(framesBack));
            }

            int target = _frame - framesBack;
            if (target < 0)
            {
                return false;
            }

            for (int i = 0; i < _ring.Length; i++)
            {
                FrameSnapshot slot = _ring[i];
                if (slot.Valid && slot.Frame == target)
                {
                    _world.Restore(slot.State);
                    _frame = target;
                    return true;
                }
            }

            return false;
        }

        public void ClearHistory()
        {
            for (int i = 0; i < _ring.Length; i++)
            {
                _ring[i].Valid = false;
            }
        }
    }
}
