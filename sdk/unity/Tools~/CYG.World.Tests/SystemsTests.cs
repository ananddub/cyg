using System;
using System.Collections.Generic;

namespace CookYourGame.Sim.Tests
{
    internal static class SystemsTests
    {
        public static void RunAll()
        {
            TestOrdering();
            TestFilterIteration();
            TestEnableDisable();
            TestLifecycle();
            TestRollbackDeterminism();
        }

        private static void TestOrdering()
        {
            CYGWorld world = new CYGWorld();
            CYGSystemRunner runner = new CYGSystemRunner(world, CYGDeterminismConfig.Default);
            List<string> log = new List<string>();

            runner.AddSystem(new RecordingSystem("late", log), order: 20);
            runner.AddSystem(new RecordingSystem("first", log), order: -10);
            runner.AddSystem(new RecordingSystem("middle-a", log), order: 5);
            runner.AddSystem(new RecordingSystem("middle-b", log), order: 5);

            runner.Tick(1);

            Equal("first,middle-a,middle-b,late", string.Join(",", log), "system order");
        }

        private static void TestFilterIteration()
        {
            CYGWorld world = new CYGWorld();
            world.Register<TestPosition>();
            world.Register<TestHealth>();

            CYGEntityRef first = world.CreateEntity();
            CYGEntityRef second = world.CreateEntity();
            CYGEntityRef third = world.CreateEntity();
            world.Set(first, new TestPosition { Value = new FPVector3(FP.One, FP.Zero, FP.Zero) });
            world.Set(second, new TestPosition { Value = new FPVector3(FP.Zero, FP.One, FP.Zero) });
            world.Set(third, new TestPosition { Value = new FPVector3(FP.Zero, FP.Zero, FP.One) });
            world.Set(second, new TestHealth { Value = FP.One, Team = 1 });

            HealthSystem system = new HealthSystem();
            CYGSystemRunner runner = new CYGSystemRunner(world, CYGDeterminismConfig.Default);
            runner.AddSystem(system);
            runner.Tick(1);

            Equal(1, system.Visited.Count, "filter must visit only matching entities");
            Equal(second.Index, system.Visited[0], "filter must preserve slot order");
            TestHealth health;
            Require(world.TryGet(second, out health), "health lookup");
            Equal(FP.FromInt(2).RawValue, health.Value.RawValue, "filtered system update");
        }

        private static void TestEnableDisable()
        {
            CYGWorld world = new CYGWorld();
            CYGSystemGroup group = new CYGSystemGroup("gameplay");
            ToggleSystem system = new ToggleSystem();
            CYGSystemRunner runner = new CYGSystemRunner(world, CYGDeterminismConfig.Default);
            runner.AddSystem(system, group);
            runner.Tick(1);
            Equal(1, system.Updates, "system runs by default");
            Equal(0, system.EnabledCallbacks, "systems start enabled without an extra callback");

            runner.SetSystemEnabled(system, false);
            runner.Tick(2);
            Equal(1, system.Updates, "disabled system must not update");
            Equal(1, system.DisabledCallbacks, "disabled callback");

            runner.SetSystemEnabled(system, true);
            runner.Tick(3);
            Equal(2, system.Updates, "re-enabled system updates");
            Equal(1, system.EnabledCallbacks, "enabled callback");

            runner.SetGroupEnabled(group, false);
            runner.Tick(4);
            Equal(2, system.Updates, "disabled group must block system");
            Equal(2, system.DisabledCallbacks, "group disable callback");

            runner.SetGroupEnabled(group, true);
            runner.Tick(5);
            Equal(3, system.Updates, "enabled group resumes system");
            Equal(2, system.EnabledCallbacks, "group enable callback");
        }

        private static void TestLifecycle()
        {
            CYGWorld world = new CYGWorld();
            ToggleSystem system = new ToggleSystem();
            CYGSystemRunner runner = new CYGSystemRunner(world, CYGDeterminismConfig.Default);
            runner.AddSystem(system);
            Equal(0, system.Inits, "system must not initialize before Start");
            runner.Start();
            runner.Start();
            Equal(1, system.Inits, "start must be idempotent");
        }

        private static void TestRollbackDeterminism()
        {
            uint first = RunRollbackScenario();
            uint second = RunRollbackScenario();
            Equal(first, second, "systems plus rollback must be deterministic");
        }

        private static uint RunRollbackScenario()
        {
            CYGWorld world = new CYGWorld();
            world.Register<TestPosition>();
            world.Register<TestHealth>();

            for (int i = 0; i < 12; i++)
            {
                CYGEntityRef entity = world.CreateEntity();
                world.Set(entity, new TestPosition { Value = new FPVector3(FP.FromInt(i), FP.Zero, FP.Zero) });
                world.Set(entity, new TestHealth { Value = FP.FromInt(10), Team = i & 1 });
            }

            CYGDeterminismConfig config = new CYGDeterminismConfig(seed: 0x51EEUL, rollbackCapacity: 16);
            CYGWorldRunner runner = new CYGWorldRunner(world, config);
            CYGSystemRunner systems = new CYGSystemRunner(world, config);
            systems.AddSystem(new MoveSystem());
            systems.AddSystem(new HealthSystem());

            for (int step = 0; step < 180; step++)
            {
                if (step > 0 && step % 41 == 0)
                {
                    Require(runner.Rollback(4), "rollback must succeed");
                }
                runner.Tick(_ => systems.Tick(runner.Frame + 1));
            }

            return runner.Checksum;
        }

        private sealed class RecordingSystem : CYGSystem
        {
            private readonly string _name;
            private readonly List<string> _log;

            public RecordingSystem(string name, List<string> log)
            {
                _name = name;
                _log = log;
            }

            public override void OnUpdate(in CYGSystemContext context)
            {
                _log.Add(_name);
            }
        }

        private sealed class HealthSystem : CYGSystemMainThreadFilter<HealthFilter>
        {
            public readonly List<int> Visited = new List<int>();

            protected override void UpdateEntity(in CYGSystemContext context, CYGEntityRef entity)
            {
                TestHealth health;
                if (!context.World.TryGet(entity, out health))
                {
                    return;
                }
                health.Value += FP.One;
                context.World.Set(entity, health);
                Visited.Add(entity.Index);
            }
        }

        private struct HealthFilter : ICYGSystemFilter
        {
            public bool Matches(CYGWorld world, CYGEntityRef entity)
            {
                return world.Has<TestHealth>(entity);
            }
        }

        private sealed class MoveSystem : CYGSystemMainThreadFilter<PositionFilter>
        {
            protected override void UpdateEntity(in CYGSystemContext context, CYGEntityRef entity)
            {
                TestPosition position;
                if (!context.World.TryGet(entity, out position))
                {
                    return;
                }

                ref CYGRngState rng = ref context.World.Rng;
                position.Value = new FPVector3(
                    position.Value.x + FP.FromRaw(rng.NextInt(-1024, 1025)),
                    position.Value.y,
                    position.Value.z + FP.FromRaw(rng.NextInt(-1024, 1025)));
                context.World.Set(entity, position);
            }
        }

        private struct PositionFilter : ICYGSystemFilter
        {
            public bool Matches(CYGWorld world, CYGEntityRef entity)
            {
                return world.Has<TestPosition>(entity);
            }
        }

        private sealed class ToggleSystem : CYGSystem
        {
            public int Updates;
            public int Inits;
            public int EnabledCallbacks;
            public int DisabledCallbacks;

            public override void OnInit(CYGWorld world)
            {
                Inits++;
            }

            public override void OnUpdate(in CYGSystemContext context)
            {
                Updates++;
            }

            public override void OnEnabled(CYGWorld world)
            {
                EnabledCallbacks++;
            }

            public override void OnDisabled(CYGWorld world)
            {
                DisabledCallbacks++;
            }
        }

        private static void Require(bool condition, string message)
        {
            if (!condition)
            {
                throw new InvalidOperationException(message);
            }
        }

        private static void Equal(int expected, int actual, string message)
        {
            if (expected != actual)
            {
                throw new InvalidOperationException($"{message}: expected {expected}, actual {actual}");
            }
        }

        private static void Equal(uint expected, uint actual, string message)
        {
            if (expected != actual)
            {
                throw new InvalidOperationException($"{message}: expected 0x{expected:X8}, actual 0x{actual:X8}");
            }
        }

        private static void Equal(long expected, long actual, string message)
        {
            if (expected != actual)
            {
                throw new InvalidOperationException($"{message}: expected {expected}, actual {actual}");
            }
        }

        private static void Equal(string expected, string actual, string message)
        {
            if (!string.Equals(expected, actual, StringComparison.Ordinal))
            {
                throw new InvalidOperationException($"{message}: expected {expected}, actual {actual}");
            }
        }
    }
}
