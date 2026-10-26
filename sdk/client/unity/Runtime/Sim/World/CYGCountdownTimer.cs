using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Deterministic countdown timer for dash cooldowns, ability charges,
    /// respawn delays and match phases. All arithmetic is fixed-point, so a
    /// timer started on two clients at the same tick expires on the same tick.
    ///
    /// The struct stores only simulation state (duration, remaining time and
    /// flags), which makes it safe to embed in an ECS component and to roll
    /// back with the rest of the world.
    /// </summary>
    [Serializable]
    public struct CYGCountdownTimer : IEquatable<CYGCountdownTimer>
    {
        public const byte RunningFlag = 1;
        public const byte LoopFlag = 2;
        public const byte ExpiredFlag = 4;

        public FP Duration;
        public FP Remaining;
        public byte Flags;

        public bool IsRunning
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return (Flags & RunningFlag) != 0; }
        }

        public bool Loop
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return (Flags & LoopFlag) != 0; }
        }

        public bool HasExpired
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return (Flags & ExpiredFlag) != 0; }
        }

        /// <summary>True while running and the remaining time is greater than zero.</summary>
        public bool IsActive
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return IsRunning && Remaining > FP.Zero; }
        }

        /// <summary>Elapsed fraction in [0,1]. Zero at start, one when expired.</summary>
        public FP Progress
        {
            get
            {
                if (Duration <= FP.Zero)
                {
                    return FP.One;
                }
                return FP.Clamp01((Duration - Remaining) / Duration);
            }
        }

        /// <summary>Remaining fraction in [0,1], convenient for fill bars.</summary>
        public FP Normalized
        {
            get
            {
                if (Duration <= FP.Zero)
                {
                    return FP.Zero;
                }
                return FP.Clamp01(Remaining / Duration);
            }
        }

        public static CYGCountdownTimer Create(FP duration, bool loop = false, bool start = true)
        {
            CYGCountdownTimer timer = new CYGCountdownTimer
            {
                Duration = FP.Max(FP.Zero, duration),
                Remaining = FP.Max(FP.Zero, duration),
            };
            timer.SetFlag(LoopFlag, loop);
            if (start)
            {
                timer.Start();
            }
            return timer;
        }

        /// <summary>Starts (or restarts) the timer from its configured duration.</summary>
        public void Start()
        {
            Remaining = Duration;
            SetFlag(ExpiredFlag, false);
            SetFlag(RunningFlag, Duration > FP.Zero);
            if (Duration <= FP.Zero)
            {
                SetFlag(ExpiredFlag, true);
            }
        }

        /// <summary>Starts the timer with a new duration.</summary>
        public void Start(FP duration)
        {
            Duration = FP.Max(FP.Zero, duration);
            Start();
        }

        /// <summary>Alias for <see cref="Start()"/> kept for gameplay readability.</summary>
        public void Restart()
        {
            Start();
        }

        public void Stop()
        {
            SetFlag(RunningFlag, false);
        }

        public void Reset()
        {
            Remaining = Duration;
            SetFlag(RunningFlag, false);
            SetFlag(ExpiredFlag, false);
        }

        public void SetLoop(bool loop)
        {
            SetFlag(LoopFlag, loop);
        }

        /// <summary>
        /// Advances the timer. Returns true exactly on the tick where the timer
        /// expires (or wraps, for looping timers).
        /// </summary>
        public bool Tick(FP deltaTime)
        {
            if (!IsRunning || deltaTime <= FP.Zero)
            {
                return false;
            }

            Remaining -= deltaTime;
            if (Remaining > FP.Zero)
            {
                return false;
            }

            SetFlag(ExpiredFlag, true);
            if (Loop && Duration > FP.Zero)
            {
                Remaining %= Duration;
                if (Remaining <= FP.Zero)
                {
                    Remaining += Duration;
                }
                return true;
            }

            Remaining = FP.Zero;
            SetFlag(RunningFlag, false);
            return true;
        }

        /// <summary>Advances without reporting the expiry edge.</summary>
        public void Advance(FP deltaTime)
        {
            Tick(deltaTime);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private void SetFlag(byte flag, bool value)
        {
            if (value)
            {
                Flags |= flag;
            }
            else
            {
                Flags = (byte)(Flags & ~flag);
            }
        }

        public bool Equals(CYGCountdownTimer other)
        {
            return Duration == other.Duration
                && Remaining == other.Remaining
                && Flags == other.Flags;
        }

        public override bool Equals(object obj)
        {
            return obj is CYGCountdownTimer other && Equals(other);
        }

        public override int GetHashCode()
        {
            return Duration.GetHashCode() ^ (Remaining.GetHashCode() * 397) ^ Flags;
        }

        public override string ToString()
        {
            return "Countdown(remaining=" + Remaining + "/" + Duration
                + (IsRunning ? ", running" : ", stopped")
                + (Loop ? ", loop" : string.Empty)
                + (HasExpired ? ", expired" : string.Empty) + ")";
        }
    }
}
