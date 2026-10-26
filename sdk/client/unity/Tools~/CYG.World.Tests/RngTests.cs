using System;

namespace CookYourGame.Sim.Tests
{
    internal static class RngTests
    {
        public static void RunAll()
        {
            TestKnownVectors();
            TestRangesAndFixedPoint();
            TestShuffle();
            TestWireRoundTrip();
            TestWorldSnapshotRestoresState();
            TestConfigContract();
        }

        private static void TestKnownVectors()
        {
            CYGRngState rng = new CYGRngState(42UL, 54UL);
            uint[] expected =
            {
                0xA15C02B7u,
                0x7B47F409u,
                0xBA1D3330u,
                0x83D2F293u,
                0xBFA4784Bu,
                0xCBED606Eu
            };

            for (int i = 0; i < expected.Length; i++)
            {
                Equal(expected[i], rng.NextUInt(), "PCG32 vector " + i);
            }

            CYGRngState same = new CYGRngState(42UL, 54UL);
            CYGRngState different = new CYGRngState(42UL, 55UL);
            rng = new CYGRngState(42UL, 54UL);
            for (int i = 0; i < 64; i++)
            {
                uint repeat = rng.NextUInt();
                Equal(repeat, same.NextUInt(), "same seed must repeat");
                if (different.NextUInt() == repeat)
                {
                    throw new InvalidOperationException("different PCG sequence produced the same output");
                }
            }
        }

        private static void TestRangesAndFixedPoint()
        {
            CYGRngState rng = new CYGRngState(1234UL, 7UL);
            for (int i = 0; i < 10000; i++)
            {
                int value = rng.NextInt(10, 20);
                Require(value >= 10 && value < 20, "NextInt range");

                uint unsigned = rng.NextUInt(5u, 9u);
                Require(unsigned >= 5u && unsigned < 9u, "NextUInt range");

                FP unit = rng.NextFP();
                Require(unit.RawValue >= 0 && unit.RawValue < FP.One.RawValue, "NextFP unit range");

                FP ranged = rng.NextFP(FP.FromInt(-2), FP.FromInt(3));
                Require(ranged >= FP.FromInt(-2) && ranged < FP.FromInt(3), "NextFP range");
            }

            bool threw = false;
            try
            {
                rng.NextInt(5, 5);
            }
            catch (ArgumentOutOfRangeException)
            {
                threw = true;
            }
            Require(threw, "empty NextInt range must throw");
        }

        private static void TestShuffle()
        {
            CYGRngState first = new CYGRngState(99UL, 3UL);
            CYGRngState second = new CYGRngState(99UL, 3UL);
            int[] left = { 0, 1, 2, 3, 4, 5, 6, 7 };
            int[] right = { 0, 1, 2, 3, 4, 5, 6, 7 };

            first.Shuffle(left);
            second.Shuffle(right);
            for (int i = 0; i < left.Length; i++)
            {
                Equal(left[i], right[i], "shuffle must be deterministic");
            }

            bool[] seen = new bool[left.Length];
            for (int i = 0; i < left.Length; i++)
            {
                int value = left[i];
                Require(value >= 0 && value < seen.Length && !seen[value], "shuffle must be a permutation");
                seen[value] = true;
            }
        }

        private static void TestWireRoundTrip()
        {
            CYGRngState rng = new CYGRngState(0xDEADBEEFUL, 17UL);
            rng.NextUInt();
            byte[] bytes = new byte[CYGRngState.SizeInBytes];
            Equal(CYGRngState.SizeInBytes, rng.WriteTo(bytes, 0), "rng wire size");

            CYGRngState restored = default;
            restored.ReadFrom(bytes, 0);
            Require(restored == rng, "rng wire round-trip");

            uint left = 0xFFFFFFFFu;
            uint right = 0xFFFFFFFFu;
            rng.ComputeChecksum(ref left);
            restored.ComputeChecksum(ref right);
            Equal(left, right, "rng checksum round-trip");
        }

        private static void TestWorldSnapshotRestoresState()
        {
            CYGWorld world = new CYGWorld(77UL, 5UL);
            for (int i = 0; i < 10; i++)
            {
                world.Rng.NextUInt();
            }

            CYGWorldSnapshot snapshot = world.CreateSnapshot();
            uint[] expected = new uint[16];
            for (int i = 0; i < expected.Length; i++)
            {
                expected[i] = world.Rng.NextUInt();
            }

            world.Rng.NextUInt();
            world.Restore(snapshot);
            Require(world.Rng == snapshot.Rng, "restore must recover rng state");
            for (int i = 0; i < expected.Length; i++)
            {
                Equal(expected[i], world.Rng.NextUInt(), "restored rng sequence " + i);
            }

            uint before = world.ComputeChecksum();
            world.Rng.NextUInt();
            Require(before != world.ComputeChecksum(), "rng progression must affect world checksum");
        }

        private static void TestConfigContract()
        {
            CYGDeterminismConfig config = CYGDeterminismConfig.Default;
            Equal(60, config.TickRate, "default tick rate");
            Equal(8, config.MaxPlayers, "default max players");
            Equal((FP.One / FP.FromInt(60)).RawValue, config.FixedDeltaTime.RawValue, "default fixed delta");

            CYGDeterminismConfig same = new CYGDeterminismConfig();
            Equal(config.ComputeFingerprint(), same.ComputeFingerprint(), "config fingerprint must be stable");

            CYGDeterminismConfig seeded = new CYGDeterminismConfig(seed: 9UL);
            Require(config.ComputeFingerprint() != seeded.ComputeFingerprint(), "seed must change fingerprint");

            bool threw = false;
            try
            {
                new CYGDeterminismConfig(tickRate: 0);
            }
            catch (ArgumentOutOfRangeException)
            {
                threw = true;
            }
            Require(threw, "invalid tick rate must throw");

            CYGWorld world = new CYGWorld();
            CYGWorldRunner runner = new CYGWorldRunner(world, new CYGDeterminismConfig(seed: 123UL, rollbackCapacity: 8));
            Equal(8, runner.RollbackCapacity, "runner capacity from config");
            Require(runner.World.Rng == new CYGRngState(123UL), "runner seeds world rng");
        }

        private static void Require(bool condition, string message)
        {
            if (!condition)
            {
                throw new InvalidOperationException(message);
            }
        }

        private static void Equal(uint expected, uint actual, string message)
        {
            if (expected != actual)
            {
                throw new InvalidOperationException($"{message}: expected 0x{expected:X8}, actual 0x{actual:X8}");
            }
        }

        private static void Equal(int expected, int actual, string message)
        {
            if (expected != actual)
            {
                throw new InvalidOperationException($"{message}: expected {expected}, actual {actual}");
            }
        }

        private static void Equal(long expected, long actual, string message)
        {
            if (expected != actual)
            {
                throw new InvalidOperationException($"{message}: expected {expected}, actual {actual}");
            }
        }
    }
}
