using System;

namespace CookYourGame.Sim.Tests
{
    internal static class DesyncDiagnosticsTests
    {
        public static void RunAll()
        {
            TestEqualSnapshots();
            TestComponentDifference();
            TestRngDifference();
            TestEntitySlotDifference();
        }

        private static void TestEqualSnapshots()
        {
            CYGWorld left = BuildWorld();
            CYGWorld right = BuildWorld();
            string report = CYGDesyncDiagnostics.Compare(10, left.CreateSnapshot(), 10, right.CreateSnapshot());
            Require(report == null, "identical snapshots must not report a desync: " + report);
        }

        private static void TestComponentDifference()
        {
            CYGWorld left = BuildWorld();
            CYGWorld right = BuildWorld();
            CYGEntityRef entity = new CYGEntityRef(1, 1);
            TestHealth health;
            Require(right.TryGet(entity, out health), "health lookup");
            health.Value += FP.One;
            right.Set(entity, health);

            string report = CYGDesyncDiagnostics.Compare(10, left.CreateSnapshot(), 10, right.CreateSnapshot());
            Require(report != null, "component difference must report a desync");
            Require(report.Contains("component 5002"), "report must name the component store: " + report);
            Require(report.Contains("slot 1"), "report must name the first divergent slot: " + report);
        }

        private static void TestRngDifference()
        {
            CYGWorld left = BuildWorld();
            CYGWorld right = BuildWorld();
            right.Rng.NextUInt();

            string report = CYGDesyncDiagnostics.Compare(10, left.CreateSnapshot(), 10, right.CreateSnapshot());
            Require(report != null && report.Contains("rng state"), "rng difference must be reported: " + report);
        }

        private static void TestEntitySlotDifference()
        {
            CYGWorld left = BuildWorld();
            CYGWorld right = BuildWorld();
            Require(right.DestroyEntity(new CYGEntityRef(2, 1)), "destroy entity");

            string report = CYGDesyncDiagnostics.Compare(10, left.CreateSnapshot(), 10, right.CreateSnapshot());
            Require(report != null, "entity difference must report a desync");
            Require(report.Contains("slot 2"), "report must name the divergent entity slot: " + report);
        }

        private static CYGWorld BuildWorld()
        {
            CYGWorld world = new CYGWorld(0xABCUL, 2UL);
            world.Register<TestPosition>();
            world.Register<TestHealth>();
            world.Register<TestLabel>();

            for (int i = 0; i < 3; i++)
            {
                CYGEntityRef entity = world.CreateEntity();
                world.Set(entity, new TestPosition
                {
                    Value = new FPVector3(FP.FromInt(i), FP.FromInt(1), FP.FromInt(2))
                });
                world.Set(entity, new TestHealth
                {
                    Value = FP.FromInt(100),
                    Team = i
                });
                world.Set(entity, new TestLabel { Id = i });
            }

            return world;
        }

        private static void Require(bool condition, string message)
        {
            if (!condition)
            {
                throw new InvalidOperationException(message);
            }
        }
    }
}
