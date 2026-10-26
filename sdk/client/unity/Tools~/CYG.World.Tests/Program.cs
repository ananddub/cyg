using System;

namespace CookYourGame.Sim.Tests
{
    [CYGComponent(5001)]
    public partial struct TestPosition
    {
        public FPVector3 Value;
    }

    [CYGComponent(5002)]
    public partial struct TestHealth
    {
        public FP Value;
        public int Team;
    }

    [CYGComponent(5003)]
    public partial struct TestLabel
    {
        public int Id;
    }

    internal static class Program
    {
        private static int _assertions;

        private static int Main()
        {
            try
            {
                TestEntityLifecycle();
                TestComponents();
                TestChecksum();
                TestSnapshotAndRestore();
                TestSnapshotReuse();
                TestRegistration();
                TestClear();
                TestRunnerRollback();
                RngTests.RunAll();
                GlobalsTests.RunAll();
                WorldRollbackSessionTests.RunAll();
                PhysicsNavigationBridgeTests.RunAll();
                DesyncDiagnosticsTests.RunAll();
                SnapshotParticipantTests.RunAll();
                DeterminismSoakTests.RunAll();
                AssemblyRulesTests.RunAll();
                SystemsTests.RunAll();
                EventsSignalsCommandsTests.RunAll();
                KccTests.RunAll();
                NetcodeTests.RunAll();
                ReplayTests.RunAll();
                DeterministicScenarioTests.RunAll();
                TwoBoxTestRunner.Run();
                ViewComponentTests.RunAll();
                FeatureTests.RunAll();

                Console.WriteLine("CYG world tests passed: assertions=" + (_assertions + EventsSignalsCommandsTests.AssertionCount + KccTests.AssertionCount + NetcodeTests.AssertionCount + ReplayTests.AssertionCount + GlobalsTests.AssertionCount + WorldRollbackSessionTests.AssertionCount + PhysicsNavigationBridgeTests.AssertionCount + TwoBoxTestRunner.AssertionCount + ViewComponentTests.AssertionCount + FeatureTests.AssertionCount));
                return 0;
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine("CYG world tests FAILED: " + ex.GetType().Name + ": " + ex.Message);
                Console.Error.WriteLine(ex.StackTrace);
                return 1;
            }
        }

        private static void TestEntityLifecycle()
        {
            CYGWorld world = new CYGWorld();
            CYGEntityRef entity = world.CreateEntity();

            Require(entity.Index == 0, "first entity should use slot 0");
            Require(entity.Version != 0, "live entity version must be non-zero");
            Require(world.Exists(entity), "new entity must exist");
            Require(!world.Exists(CYGEntityRef.None), "None must not exist");
            Require(!world.Exists(new CYGEntityRef(0, entity.Version + 1)), "wrong version must not exist");
            Equal(1, world.EntityCount, "entity count");

            Require(world.DestroyEntity(entity), "destroy live entity");
            Require(!world.Exists(entity), "destroyed entity must not exist");
            Equal(0, world.EntityCount, "entity count after destroy");
            Require(!world.DestroyEntity(entity), "destroying twice must return false");

            CYGEntityRef reused = world.CreateEntity();
            Equal(entity.Index, reused.Index, "destroyed slot should be reused");
            Require(reused.Version != entity.Version, "reused slot must bump the version");
            Require(world.Exists(reused), "reused entity must exist");
            Require(!world.Exists(entity), "stale reference must stay invalid after reuse");
        }

        private static void TestComponents()
        {
            CYGWorld world = new CYGWorld();
            world.Register<TestPosition>();
            world.Register<TestHealth>();
            world.Register<TestLabel>();

            CYGEntityRef entity = world.CreateEntity();
            uint before = world.StructuralVersion;

            TestPosition position = new TestPosition { Value = new FPVector3(FP.FromInt(1), FP.FromInt(2), FP.FromInt(3)) };
            world.Set(entity, position);
            Require(world.StructuralVersion > before, "adding a component must bump StructuralVersion");
            Require(world.Has<TestPosition>(entity), "entity should have TestPosition");

            TestPosition read;
            Require(world.TryGet(entity, out read), "TryGet should find TestPosition");
            Equal(Raw(1), read.Value.x.RawValue, "position x");

            TestPosition updated = new TestPosition { Value = new FPVector3(FP.FromInt(4), FP.FromInt(5), FP.FromInt(6)) };
            world.Set(entity, updated);
            Require(world.TryGet(entity, out read), "TryGet should still find TestPosition");
            Equal(Raw(6), read.Value.z.RawValue, "updated position z");

            TestHealth health = new TestHealth { Value = FP.FromInt(100), Team = 2 };
            world.Set(entity, health);
            TestHealth healthRead;
            Require(world.TryGet(entity, out healthRead), "TryGet should find TestHealth");
            Equal(2, healthRead.Team, "health team");
            Require(world.TryGetStore(5002, out ICYGComponentStore store), "TryGetStore should find TestHealth");
            Equal(1, store.Count, "store count");

            uint beforeRemove = world.StructuralVersion;
            Require(world.Remove<TestPosition>(entity), "Remove should remove TestPosition");
            Require(world.StructuralVersion > beforeRemove, "removing a component must bump StructuralVersion");
            Require(!world.Has<TestPosition>(entity), "removed component must be absent");
            Require(!world.Remove<TestPosition>(entity), "removing twice must return false");
            Require(world.Has<TestHealth>(entity), "other components must survive removal");

            Require(world.DestroyEntity(entity), "destroy entity with components");
            TestHealth missing;
            Require(!world.TryGet(entity, out missing), "destroyed entity must lose components");

            bool threw = false;
            try
            {
                world.Set(entity, position);
            }
            catch (InvalidOperationException)
            {
                threw = true;
            }
            Require(threw, "Set on a dead entity must throw");
        }

        private static void TestChecksum()
        {
            CYGWorld first = BuildSampleWorld();
            CYGWorld second = BuildSampleWorld();

            Equal(first.ComputeChecksum(), second.ComputeChecksum(), "identical worlds must checksum equal");

            CYGEntityRef entity = new CYGEntityRef(0, 1);
            TestHealth health;
            Require(second.TryGet(entity, out health), "sample entity health");
            health.Team = 9;
            second.Set(entity, health);
            Require(first.ComputeChecksum() != second.ComputeChecksum(), "changed component must change checksum");

            CYGWorld third = BuildSampleWorld();
            third.CreateEntity();
            Require(first.ComputeChecksum() != third.ComputeChecksum(), "new entity must change checksum");
        }

        private static void TestSnapshotAndRestore()
        {
            CYGWorld world = BuildSampleWorld();
            uint original = world.ComputeChecksum();
            CYGWorldSnapshot snapshot = world.CreateSnapshot();
            Equal(2, snapshot.EntityCount, "snapshot entity count");
            Equal(3, snapshot.StoreCount, "snapshot store count");

            CYGEntityRef first = new CYGEntityRef(0, 1);
            TestHealth health;
            Require(world.TryGet(first, out health), "health before snapshot mutation");
            health.Team = 7;
            world.Set(first, health);
            world.Remove<TestLabel>(first);
            world.CreateEntity();
            world.DestroyEntity(new CYGEntityRef(1, 1));

            Require(world.ComputeChecksum() != original, "world must differ after mutations");

            world.Restore(snapshot);
            Equal(original, world.ComputeChecksum(), "restore must reproduce the captured checksum");
            Equal(2, world.EntityCount, "restored entity count");
            Require(world.Exists(first), "restored first entity");
            Require(world.Exists(new CYGEntityRef(1, 1)), "restored second entity");
            Require(world.TryGet(first, out health), "restored health");
            Equal(2, health.Team, "restored health value");
            Require(world.Has<TestLabel>(first), "restored component membership");
        }

        private static void TestSnapshotReuse()
        {
            CYGWorld world = BuildSampleWorld();
            CYGWorldSnapshot snapshot = world.CreateSnapshot();

            CYGEntityRef first = new CYGEntityRef(0, 1);
            TestHealth health;
            Require(world.TryGet(first, out health), "health for reuse snapshot");
            health.Team = 4;
            world.Set(first, health);
            world.Capture(snapshot);
            uint second = world.ComputeChecksum();

            health.Team = 5;
            world.Set(first, health);
            Require(world.ComputeChecksum() != second, "mutation after recapture");

            world.Restore(snapshot);
            Equal(second, world.ComputeChecksum(), "recaptured snapshot must restore the second state");
            Require(world.TryGet(first, out health), "health after reuse restore");
            Equal(4, health.Team, "reused snapshot value");

            bool threw = false;
            try
            {
                world.Register<TestExtra>();
                world.Restore(snapshot);
            }
            catch (InvalidOperationException)
            {
                threw = true;
            }
            Require(threw, "restoring into a world with a new store must fail");
        }

        private static void TestRegistration()
        {
            CYGWorld world = new CYGWorld();
            CYGEntityRef a = world.CreateEntity();
            CYGEntityRef b = world.CreateEntity();
            world.Register<TestPosition>();

            world.Set(a, new TestPosition { Value = new FPVector3(FP.FromInt(1), FP.FromInt(1), FP.FromInt(1)) });
            world.Set(b, new TestPosition { Value = new FPVector3(FP.FromInt(2), FP.FromInt(2), FP.FromInt(2)) });
            TestPosition value;
            Require(world.TryGet(b, out value), "late registration must still store components");
            Equal(Raw(2), value.Value.x.RawValue, "late registration value");

            CYGWorldSnapshot snapshot = world.CreateSnapshot();
            Equal(1, snapshot.StoreCount, "late registration snapshot store count");
            world.Set(a, new TestPosition { Value = new FPVector3(FP.FromInt(9), FP.FromInt(9), FP.FromInt(9)) });
            world.Restore(snapshot);
            Require(world.TryGet(a, out value), "late registration restore");
            Equal(Raw(1), value.Value.x.RawValue, "late registration restored value");

            bool duplicate = false;
            try
            {
                world.Register<TestPosition>();
            }
            catch (InvalidOperationException)
            {
                duplicate = true;
            }
            Require(duplicate, "duplicate store registration must throw");

            bool missing = false;
            try
            {
                world.Set(a, new TestExtra { Value = 1 });
            }
            catch (InvalidOperationException)
            {
                missing = true;
            }
            Require(missing, "setting an unregistered component must throw");
            Require(!world.Has<TestExtra>(a), "unregistered component must not be reported");
        }

        private static void TestClear()
        {
            CYGWorld world = new CYGWorld();
            world.Register<TestPosition>();
            CYGEntityRef first = world.CreateEntity();
            world.Set(first, PositionAt(1));
            world.Clear();
            Require(!world.Exists(first), "Clear must invalidate references");
            Equal(0, world.EntityCount, "Clear entity count");
            Require(world.Stores[0].Count == 0, "Clear must empty component stores");

            CYGEntityRef recreated = world.CreateEntity();
            Equal(first.Index, recreated.Index, "Clear should recycle slots");
            Require(recreated.Version != first.Version, "Clear must bump slot versions");
            Require(!world.Exists(first), "old reference must stay invalid after Clear");
        }

        private static void TestRunnerRollback()
        {
            CYGWorld world = new CYGWorld();
            world.Register<TestPosition>();
            CYGEntityRef entity = world.CreateEntity();

            CYGWorldRunner runner = new CYGWorldRunner(world, 8);
            Equal(0, runner.Frame, "runner starts at frame 0");

            runner.Tick(w => w.Set(entity, PositionAt(1)));
            uint atOne = runner.Checksum;
            runner.Tick(w => w.Set(entity, PositionAt(2)));
            runner.Tick(w => w.Set(entity, PositionAt(3)));
            Equal(3, runner.Frame, "runner frame after three ticks");

            Require(runner.Rollback(2), "rollback two frames");
            Equal(1, runner.Frame, "frame after rollback");
            Equal(atOne, runner.Checksum, "rollback must restore the earlier checksum");
            TestPosition position;
            Require(world.TryGet(entity, out position), "position after rollback");
            Equal(Raw(1), position.Value.x.RawValue, "position x after rollback");

            Require(!runner.Rollback(5), "rollback beyond history must fail");

            runner.Tick(w => w.Set(entity, PositionAt(4)));
            Equal(2, runner.Frame, "frame after re-simulation");
            Require(world.TryGet(entity, out position), "position after re-simulation");
            Equal(Raw(4), position.Value.x.RawValue, "re-simulated position");

            CYGWorldRunner tooSmall;
            bool threw = false;
            try
            {
                tooSmall = new CYGWorldRunner(world, 0);
            }
            catch (ArgumentOutOfRangeException)
            {
                threw = true;
                tooSmall = null;
            }
            Require(threw && tooSmall == null, "zero rollback capacity must throw");
        }

        private static CYGWorld BuildSampleWorld()
        {
            CYGWorld world = new CYGWorld();
            world.Register<TestPosition>();
            world.Register<TestHealth>();
            world.Register<TestLabel>();

            CYGEntityRef first = world.CreateEntity();
            CYGEntityRef second = world.CreateEntity();
            world.Set(first, new TestPosition { Value = new FPVector3(FP.FromInt(1), FP.FromInt(0), FP.FromInt(0)) });
            world.Set(first, new TestHealth { Value = FP.FromInt(100), Team = 2 });
            world.Set(first, new TestLabel { Id = 11 });
            world.Set(second, new TestPosition { Value = new FPVector3(FP.FromInt(2), FP.FromInt(0), FP.FromInt(0)) });
            world.Set(second, new TestHealth { Value = FP.FromInt(50), Team = 1 });
            return world;
        }

        private static TestPosition PositionAt(int x)
        {
            return new TestPosition { Value = new FPVector3(FP.FromInt(x), FP.FromInt(0), FP.FromInt(0)) };
        }

        private static long Raw(int value)
        {
            return FP.FromInt(value).RawValue;
        }

        private static void Require(bool condition, string message)
        {
            _assertions++;
            if (!condition)
            {
                throw new InvalidOperationException(message);
            }
        }

        private static void Equal<T>(T expected, T actual, string message)
        {
            _assertions++;
            if (!Equals(expected, actual))
            {
                throw new InvalidOperationException(message + ": expected " + expected + ", got " + actual);
            }
        }
    }

    [CYGComponent(5004)]
    public partial struct TestExtra
    {
        public int Value;
    }
}
