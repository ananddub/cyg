using System;

namespace CookYourGame.Sim.Tests
{
    internal static class ViewComponentTests
    {
        private static int _assertions;

        public static int AssertionCount
        {
            get { return _assertions; }
        }

        public static void RunAll()
        {
            TestCodecAndChecksum();
            TestSnapshotRestore();
            TestCreateBehaviourAndFlags();
        }

        private static void TestCodecAndChecksum()
        {
            CYGViewBinding view = CYGViewBinding.Create(
                new CYGAssetId(12345),
                CYGViewBinding.BindPredicted,
                CYGViewBinding.FlagPool | CYGViewBinding.FlagVisible);

            byte[] buffer = new byte[CYGViewBinding.SizeInBytes];
            Equal(CYGViewBinding.SizeInBytes, view.WriteTo(buffer, 0), "view write size");

            CYGViewBinding read = default(CYGViewBinding);
            read.ReadFrom(buffer, 0);
            Equal(12345, read.Prefab.Id, "view prefab roundtrip");
            Equal(CYGViewBinding.BindPredicted, read.CreateBehaviour, "view create behaviour roundtrip");
            Equal((byte)(CYGViewBinding.FlagPool | CYGViewBinding.FlagVisible), read.Flags, "view flags roundtrip");

            uint first = 0;
            view.ComputeChecksum(ref first);
            read.Prefab = new CYGAssetId(54321);
            uint second = 0;
            read.ComputeChecksum(ref second);
            Require(first != second, "changing the prefab changes the view checksum");
            Equal(-1030, view.ComponentTypeId, "view type id");
            Equal(6, view.WireSize, "view wire size");
        }

        private static void TestSnapshotRestore()
        {
            CYGWorld world = new CYGWorld();
            world.Register<CYGViewBinding>();
            CYGEntityRef entity = world.CreateEntity();
            world.Set(entity, CYGViewBinding.Create(new CYGAssetId(7)));

            CYGWorldSnapshot snapshot = world.CreateSnapshot();
            world.Capture(snapshot);

            world.Set(entity, CYGViewBinding.Create(new CYGAssetId(99)));
            CYGViewBinding changed;
            Require(world.TryGet(entity, out changed), "view exists after update");
            Equal(99, changed.Prefab.Id, "view updated before restore");

            world.Restore(snapshot);
            CYGViewBinding restored;
            Require(world.TryGet(entity, out restored), "view exists after restore");
            Equal(7, restored.Prefab.Id, "view restored from snapshot");
        }

        private static void TestCreateBehaviourAndFlags()
        {
            CYGViewBinding defaults = CYGViewBinding.Create(new CYGAssetId(1));
            Equal(CYGViewBinding.BindVerified, defaults.CreateBehaviour, "default create behaviour");
            Equal((byte)(CYGViewBinding.FlagPool | CYGViewBinding.FlagVisible), defaults.Flags, "default view flags");

            CYGWorld world = new CYGWorld();
            world.Register<CYGViewBinding>();
            CYGEntityRef entity = world.CreateEntity();
            uint before = world.ComputeChecksum();
            world.Set(entity, CYGViewBinding.Create(new CYGAssetId(2)));
            Require(world.ComputeChecksum() != before, "registering a view changes the world checksum");
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
            Require(Equals(expected, actual), message + " (expected " + expected + ", got " + actual + ")");
        }
    }
}
