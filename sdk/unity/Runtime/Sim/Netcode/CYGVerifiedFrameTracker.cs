using System;

namespace CookYourGame.Sim
{
    public readonly struct CYGDesyncInfo
    {
        public readonly int Tick;
        public readonly uint LocalChecksum;
        public readonly uint RemoteChecksum;

        public CYGDesyncInfo(int tick, uint localChecksum, uint remoteChecksum)
        {
            Tick = tick;
            LocalChecksum = localChecksum;
            RemoteChecksum = remoteChecksum;
        }

        public override string ToString()
        {
            return "Desync at tick " + Tick
                + ": local=0x" + LocalChecksum.ToString("X8")
                + " remote=0x" + RemoteChecksum.ToString("X8");
        }
    }

    /// <summary>
    /// Tracks predicted local checksums against server-authoritative checksums.
    /// A mismatch is recorded at the tick where it was first observed; the
    /// caller can then run <see cref="CYGDesyncDiagnostics"/> on stored
    /// snapshots to locate the divergent component slot.
    /// </summary>
    public sealed class CYGVerifiedFrameTracker
    {
        private struct Frame
        {
            public int Tick;
            public uint Checksum;
            public bool HasLocal;
            public bool Verified;
        }

        private readonly Frame[] _frames;
        private readonly int _capacity;

        public CYGVerifiedFrameTracker(int capacity = 256)
        {
            if (capacity < 2)
            {
                throw new ArgumentOutOfRangeException(nameof(capacity));
            }
            _capacity = capacity;
            _frames = new Frame[capacity];
            for (int i = 0; i < capacity; i++)
            {
                _frames[i].Tick = -1;
            }
        }

        public event Action<CYGDesyncInfo> OnDesync;

        public uint LastVerifiedChecksum { get; private set; }

        public int LastVerifiedTick { get; private set; } = -1;

        public bool HasDesync { get; private set; }

        public CYGDesyncInfo FirstDesync { get; private set; }

        public int MismatchCount { get; private set; }

        /// <summary>Number of predicted frames that have not been confirmed yet.</summary>
        public int PendingCount
        {
            get
            {
                int count = 0;
                for (int i = 0; i < _frames.Length; i++)
                {
                    if (_frames[i].Tick >= 0 && _frames[i].HasLocal && !_frames[i].Verified)
                    {
                        count++;
                    }
                }
                return count;
            }
        }

        public void MarkPredicted(int tick, uint localChecksum)
        {
            RequireTick(tick);
            ref Frame frame = ref _frames[tick % _capacity];
            if (frame.Tick != tick)
            {
                frame = new Frame { Tick = tick };
            }
            frame.Checksum = localChecksum;
            frame.HasLocal = true;
            frame.Verified = false;
        }

        /// <summary>
        /// Confirms a tick against the server checksum. Returns false on a
        /// mismatch; the first mismatch is kept in <see cref="FirstDesync"/>.
        /// </summary>
        public bool MarkVerified(int tick, uint remoteChecksum)
        {
            RequireTick(tick);
            ref Frame frame = ref _frames[tick % _capacity];
            if (frame.Tick != tick)
            {
                frame = new Frame { Tick = tick };
            }

            if (tick > LastVerifiedTick)
            {
                LastVerifiedTick = tick;
                LastVerifiedChecksum = remoteChecksum;
            }

            if (!frame.HasLocal)
            {
                frame.Checksum = remoteChecksum;
                frame.HasLocal = true;
                frame.Verified = true;
                return true;
            }

            if (frame.Checksum != remoteChecksum)
            {
                CYGDesyncInfo info = new CYGDesyncInfo(tick, frame.Checksum, remoteChecksum);
                MismatchCount++;
                if (!HasDesync)
                {
                    HasDesync = true;
                    FirstDesync = info;
                }
                frame.Verified = true;
                OnDesync?.Invoke(info);
                return false;
            }

            frame.Verified = true;
            return true;
        }

        public bool TryGetLocalChecksum(int tick, out uint checksum)
        {
            RequireTick(tick);
            ref Frame frame = ref _frames[tick % _capacity];
            if (frame.Tick == tick && frame.HasLocal)
            {
                checksum = frame.Checksum;
                return true;
            }
            checksum = 0u;
            return false;
        }

        public bool IsVerified(int tick)
        {
            RequireTick(tick);
            ref Frame frame = ref _frames[tick % _capacity];
            return frame.Tick == tick && frame.HasLocal && frame.Verified;
        }

        public void Clear()
        {
            for (int i = 0; i < _frames.Length; i++)
            {
                _frames[i] = new Frame { Tick = -1 };
            }
            LastVerifiedChecksum = 0u;
            LastVerifiedTick = -1;
            HasDesync = false;
            FirstDesync = default(CYGDesyncInfo);
            MismatchCount = 0;
        }

        private static void RequireTick(int tick)
        {
            if (tick < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(tick));
            }
        }
    }
}
