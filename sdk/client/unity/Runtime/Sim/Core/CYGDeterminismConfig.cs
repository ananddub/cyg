using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Immutable simulation contract shared by client and server. Changing any
    /// value changes the run, so the fingerprint is exchanged during session
    /// setup and checked before the first tick.
    /// </summary>
    public sealed class CYGDeterminismConfig
    {
        public const int DefaultTickRate = 60;
        public const int DefaultMaxPlayers = 8;
        public const ulong DefaultSeed = 0UL;
        public const int DefaultChecksumInterval = 30;
        public const int DefaultRollbackCapacity = 64;
        public const int DefaultMaxPredictionTicks = 10;

        public static readonly CYGDeterminismConfig Default = new CYGDeterminismConfig();

        public CYGDeterminismConfig(
            int tickRate = DefaultTickRate,
            int maxPlayers = DefaultMaxPlayers,
            ulong seed = DefaultSeed,
            int checksumInterval = DefaultChecksumInterval,
            int rollbackCapacity = DefaultRollbackCapacity,
            int maxPredictionTicks = DefaultMaxPredictionTicks)
        {
            if (tickRate < 1 || tickRate > 1000)
            {
                throw new ArgumentOutOfRangeException(nameof(tickRate), "tickRate must be in [1, 1000].");
            }
            if (maxPlayers < 1 || maxPlayers > 256)
            {
                throw new ArgumentOutOfRangeException(nameof(maxPlayers), "maxPlayers must be in [1, 256].");
            }
            if (checksumInterval < 1)
            {
                throw new ArgumentOutOfRangeException(nameof(checksumInterval), "checksumInterval must be at least 1.");
            }
            if (rollbackCapacity < 1)
            {
                throw new ArgumentOutOfRangeException(nameof(rollbackCapacity), "rollbackCapacity must be at least 1.");
            }
            if (maxPredictionTicks < 1)
            {
                throw new ArgumentOutOfRangeException(nameof(maxPredictionTicks), "maxPredictionTicks must be at least 1.");
            }

            TickRate = tickRate;
            MaxPlayers = maxPlayers;
            Seed = seed;
            ChecksumInterval = checksumInterval;
            RollbackCapacity = rollbackCapacity;
            MaxPredictionTicks = maxPredictionTicks;
            FixedDeltaTime = FP.One / FP.FromInt(tickRate);
        }

        public int TickRate { get; }

        public FP FixedDeltaTime { get; }

        public int MaxPlayers { get; }

        public ulong Seed { get; }

        public int ChecksumInterval { get; }

        public int RollbackCapacity { get; }

        public int MaxPredictionTicks { get; }

        /// <summary>Stable hash of all settings that affect simulation results.</summary>
        public uint ComputeFingerprint()
        {
            uint crc = 0xFFFFFFFFu;
            CYGChecksum.HashInt(ref crc, TickRate);
            CYGChecksum.HashInt(ref crc, MaxPlayers);
            CYGChecksum.HashULong(ref crc, Seed);
            CYGChecksum.HashInt(ref crc, ChecksumInterval);
            CYGChecksum.HashInt(ref crc, RollbackCapacity);
            CYGChecksum.HashInt(ref crc, MaxPredictionTicks);
            return crc ^ 0xFFFFFFFFu;
        }
    }
}
