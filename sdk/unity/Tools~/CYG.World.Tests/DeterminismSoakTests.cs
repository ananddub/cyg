using System;
using System.Diagnostics;

namespace CookYourGame.Sim.Tests
{
    /// <summary>
    /// Long-run determinism and allocation guard. The 1M-tick checksum is
    /// pinned and must match on every OS in the CI matrix; the allocation test
    /// keeps the steady tick path free of managed allocations.
    /// </summary>
    internal static class DeterminismSoakTests
    {
        private const int DefaultTicks = 1000000;
        private const int ZeroAllocationWarmup = 4096;
        // Ten minutes at 60 Hz.
        private const int ZeroAllocationTicks = 36000;

        /// <summary>Pinned checksum for <see cref="DefaultTicks"/> ticks.</summary>
        private const uint PinnedChecksum = 0x7C1FAFFFu;

        public static void RunAll()
        {
            MillionTickChecksum();
            ZeroAllocationSteadyState();
        }

        private static void MillionTickChecksum()
        {
            int ticks = ReadTickOverride();
            SoakScenario scenario = new SoakScenario();
            Stopwatch watch = Stopwatch.StartNew();

            for (int i = 0; i < ticks; i++)
            {
                if (i > 0 && i % 10007 == 0)
                {
                    if (!scenario.Runner.Rollback(4))
                    {
                        throw new InvalidOperationException("soak rollback window unexpectedly expired at tick " + i);
                    }
                }
                scenario.Runner.Tick(scenario.Step);
            }

            watch.Stop();
            uint checksum = scenario.Runner.Checksum;
            Console.WriteLine(
                "soak ticks=" + ticks + " checksum=0x" + checksum.ToString("X8")
                + " elapsedMs=" + watch.ElapsedMilliseconds);

            if (PinnedChecksum != 0u && checksum != PinnedChecksum)
            {
                throw new InvalidOperationException(
                    $"soak checksum changed: expected 0x{PinnedChecksum:X8}, actual 0x{checksum:X8}");
            }
        }

        private static void ZeroAllocationSteadyState()
        {
            SoakScenario scenario = new SoakScenario();
            for (int i = 0; i < ZeroAllocationWarmup; i++)
            {
                scenario.Runner.Tick(scenario.Step);
            }

            // Run the measurement twice: the first pass can still absorb one
            // time tiered-JIT or delegate-cache cost, the second must be clean.
            MeasureTicks(scenario);
            long allocated = MeasureTicks(scenario);
            Console.WriteLine("steady allocation bytes=" + allocated + " over " + ZeroAllocationTicks + " ticks");
            if (allocated != 0)
            {
                throw new InvalidOperationException(
                    "tick path allocated " + allocated + " bytes over " + ZeroAllocationTicks + " ticks");
            }
        }

        private static long MeasureTicks(SoakScenario scenario)
        {
            long before = GC.GetAllocatedBytesForCurrentThread();
            for (int i = 0; i < ZeroAllocationTicks; i++)
            {
                scenario.Runner.Tick(scenario.Step);
            }
            return GC.GetAllocatedBytesForCurrentThread() - before;
        }

        private static int ReadTickOverride()
        {
            string value = Environment.GetEnvironmentVariable("CYG_SOAK_TICKS");
            int ticks;
            if (!string.IsNullOrEmpty(value)
                && int.TryParse(value, out ticks)
                && ticks > 0)
            {
                return ticks;
            }
            return DefaultTicks;
        }

        private sealed class SoakScenario
        {
            private readonly CYGEntityRef[] _entities = new CYGEntityRef[8];

            public SoakScenario()
            {
                CYGWorld world = new CYGWorld();
                world.Register<TestPosition>();
                world.Register<TestHealth>();
                world.Register<TestLabel>();

                for (int i = 0; i < _entities.Length; i++)
                {
                    CYGEntityRef entity = world.CreateEntity();
                    _entities[i] = entity;
                    world.Set(entity, new TestPosition
                    {
                        Value = new FPVector3(FP.FromInt(i), FP.Zero, FP.FromInt(-i))
                    });
                    world.Set(entity, new TestHealth
                    {
                        Value = FP.FromInt(100 + i),
                        Team = i & 3
                    });
                    world.Set(entity, new TestLabel { Id = i });
                }

                Runner = new CYGWorldRunner(
                    world,
                    new CYGDeterminismConfig(
                        tickRate: 60,
                        maxPlayers: 4,
                        seed: 0x50ACUL,
                        checksumInterval: 30,
                        rollbackCapacity: 64,
                        maxPredictionTicks: 8));
                Step = StepTick;
            }

            public CYGWorldRunner Runner { get; }

            public Action<CYGWorld, int> Step { get; }

            private void StepTick(CYGWorld world, int tick)
            {
                FP drift = FP.FromRaw((tick & 0xFF) + 1);
                for (int i = 0; i < _entities.Length; i++)
                {
                    CYGEntityRef entity = _entities[i];
                    TestPosition position;
                    if (!world.TryGet(entity, out position))
                    {
                        continue;
                    }

                    position.Value = new FPVector3(
                        position.Value.x + drift,
                        position.Value.y,
                        position.Value.z - drift);
                    world.Set(entity, position);
                }
            }
        }
    }
}
