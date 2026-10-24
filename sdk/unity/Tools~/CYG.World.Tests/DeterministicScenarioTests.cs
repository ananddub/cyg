using System;

namespace CookYourGame.Sim.Tests
{
    internal static class DeterministicScenarioTests
    {
        private const int EntityCount = 24;
        private const int Ticks = 600;
        private const uint PinnedChecksum = 0x5E11987Cu;

        public static void RunAll()
        {
            uint checksum = RunScenario();
            Console.WriteLine("deterministic scenario checksum=0x" + checksum.ToString("X8"));
            if (PinnedChecksum != 0u && checksum != PinnedChecksum)
            {
                throw new InvalidOperationException(
                    $"scenario checksum changed: expected 0x{PinnedChecksum:X8}, actual 0x{checksum:X8}");
            }
        }

        private static uint RunScenario()
        {
            CYGWorld world = new CYGWorld();
            world.Register<TestPosition>();
            world.Register<TestHealth>();
            world.Register<TestLabel>();

            CYGWorldRunner runner = new CYGWorldRunner(
                world,
                new CYGDeterminismConfig(
                    tickRate: 60,
                    maxPlayers: 4,
                    seed: 0x5EEDUL,
                    checksumInterval: 10,
                    rollbackCapacity: 32,
                    maxPredictionTicks: 8));

            CYGEntityRef[] entities = new CYGEntityRef[EntityCount];
            for (int i = 0; i < EntityCount; i++)
            {
                CYGEntityRef entity = world.CreateEntity();
                entities[i] = entity;
                world.Set(entity, new TestPosition
                {
                    Value = new FPVector3(FP.FromInt(i), FP.FromInt(1), FP.FromInt(-i))
                });
                world.Set(entity, new TestHealth
                {
                    Value = FP.FromInt(100),
                    Team = i & 3
                });
                world.Set(entity, new TestLabel { Id = i });
            }

            for (int step = 0; step < Ticks; step++)
            {
                if (step > 0 && step % 53 == 0)
                {
                    if (!runner.Rollback(4))
                    {
                        throw new InvalidOperationException("rollback window unexpectedly expired");
                    }
                }

                int tick = runner.Frame + 1;
                runner.Tick(w => Simulate(w, entities, tick));
            }

            return runner.Checksum;
        }

        private static void Simulate(CYGWorld world, CYGEntityRef[] entities, int tick)
        {
            ref CYGRngState rng = ref world.Rng;

            for (int i = 0; i < entities.Length; i++)
            {
                CYGEntityRef entity = entities[i];
                if (!world.Exists(entity))
                {
                    continue;
                }

                if (tick > 30 && (tick + i) % 97 == 0)
                {
                    world.DestroyEntity(entity);
                    continue;
                }

                TestPosition position;
                if (!world.TryGet(entity, out position))
                {
                    continue;
                }

                position.Value = new FPVector3(
                    position.Value.x + FP.FromRaw(rng.NextInt(-4096, 4097)),
                    position.Value.y + FP.FromRaw(rng.NextInt(-1024, 1025)),
                    position.Value.z + FP.FromRaw(rng.NextInt(-4096, 4097)));
                world.Set(entity, position);

                if (tick % 11 == 0)
                {
                    TestHealth health;
                    if (world.TryGet(entity, out health))
                    {
                        health.Value += FP.FromRaw(rng.NextInt(0, 257));
                        world.Set(entity, health);
                    }
                }
            }
        }
    }
}
