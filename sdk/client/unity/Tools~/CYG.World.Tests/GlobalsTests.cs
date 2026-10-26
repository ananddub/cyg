using System;

namespace CookYourGame.Sim.Tests
{
    internal struct TestGlobals : ICYGValue
    {
        public const int SizeInBytes = 12;

        public int Score;
        public FP Timer;

        public int WireSize
        {
            get { return SizeInBytes; }
        }

        public int WriteTo(byte[] buffer, int offset)
        {
            CYGWire.RequireWritable(buffer, offset, SizeInBytes);
            CYGWire.WriteInt32(buffer, offset, Score);
            Timer.WriteTo(buffer, offset + 4);
            return SizeInBytes;
        }

        public void ReadFrom(byte[] buffer, int offset)
        {
            CYGWire.RequireReadable(buffer, offset, SizeInBytes);
            Score = CYGWire.ReadInt32(buffer, offset);
            Timer.ReadFrom(buffer, offset + 4);
        }

        public void ComputeChecksum(ref uint crc)
        {
            CYGChecksum.HashInt(ref crc, Score);
            Timer.ComputeChecksum(ref crc);
        }
    }

    internal static class GlobalsTests
    {
        private static int _assertions;

        public static int AssertionCount
        {
            get { return _assertions; }
        }

        public static void RunAll()
        {
            TestHosting();
            TestChecksum();
            TestSnapshotRestore();
            TestMismatchAndClear();
            TestCaptureAllocation();
        }

        private static void TestHosting()
        {
            CYGWorld world = new CYGWorld();
            Require(!world.HasGlobals, "world starts without globals");
            Equal(0, world.GlobalsSize, "empty globals size");

            TestGlobals initial = new TestGlobals
            {
                Score = 42,
                Timer = FP.FromInt(3),
            };
            world.SetGlobals(initial);

            Require(world.HasGlobals, "globals installed");
            Equal(typeof(TestGlobals), world.GlobalsType, "globals type");
            Equal(TestGlobals.SizeInBytes, world.GlobalsSize, "globals wire size");

            TestGlobals read;
            Require(world.TryGetGlobals(out read), "TryGetGlobals succeeds");
            Equal(42, read.Score, "global score");
            Equal(FP.FromInt(3), read.Timer, "global timer");

            world.GetGlobals<TestGlobals>().Score = 7;
            Require(world.TryGetGlobals(out read), "TryGetGlobals after ref mutation");
            Equal(7, read.Score, "ref return mutates the hosted value");

            bool threw = false;
            try
            {
                world.SetGlobals(new OtherGlobals { Value = 1 });
            }
            catch (InvalidOperationException)
            {
                threw = true;
            }
            Require(threw, "switching global schema throws");
        }

        private static void TestChecksum()
        {
            CYGTestWorldFactory factory = new CYGTestWorldFactory();
            CYGWorld first = factory.Create();
            CYGWorld second = factory.Create();

            Equal(first.ComputeChecksum(), second.ComputeChecksum(), "identical globals checksum equal");

            first.SetGlobals(new TestGlobals { Score = 1 });
            Require(first.ComputeChecksum() != second.ComputeChecksum(), "adding globals changes checksum");

            second.SetGlobals(new TestGlobals { Score = 1 });
            Equal(first.ComputeChecksum(), second.ComputeChecksum(), "equal global values checksum equal");

            second.GetGlobals<TestGlobals>().Score = 2;
            Require(first.ComputeChecksum() != second.ComputeChecksum(), "global value change changes checksum");
        }

        private static void TestSnapshotRestore()
        {
            CYGWorld world = new CYGWorld();
            world.SetGlobals(new TestGlobals { Score = 5, Timer = FP.FromInt(2) });
            world.CreateEntity();
            uint expected = world.ComputeChecksum();

            CYGWorldSnapshot snapshot = world.CreateSnapshot();
            world.GetGlobals<TestGlobals>().Score = 99;
            world.CreateEntity();
            Require(world.ComputeChecksum() != expected, "mutations change checksum");

            world.Restore(snapshot);
            Equal(expected, world.ComputeChecksum(), "restore reproduces the checksum");
            TestGlobals restored;
            Require(world.TryGetGlobals(out restored), "globals restored");
            Equal(5, restored.Score, "restored global score");
            Equal(FP.FromInt(2), restored.Timer, "restored global timer");
        }

        private static void TestMismatchAndClear()
        {
            CYGWorld withGlobals = new CYGWorld();
            withGlobals.SetGlobals(new TestGlobals { Score = 3 });
            CYGWorldSnapshot globalsSnapshot = withGlobals.CreateSnapshot();

            CYGWorld withoutGlobals = new CYGWorld();
            bool threw = false;
            try
            {
                withoutGlobals.Restore(globalsSnapshot);
            }
            catch (InvalidOperationException)
            {
                threw = true;
            }
            Require(threw, "restoring globals into a world without them throws");

            CYGWorld emptyWorld = new CYGWorld();
            CYGWorldSnapshot emptySnapshot = emptyWorld.CreateSnapshot();
            CYGWorld target = new CYGWorld();
            target.SetGlobals(new TestGlobals { Score = 8 });
            target.Restore(emptySnapshot);
            TestGlobals reset;
            Require(target.TryGetGlobals(out reset), "globals remain registered after empty restore");
            Equal(0, reset.Score, "empty snapshot resets globals to default");

            target.GetGlobals<TestGlobals>().Score = 11;
            target.Clear();
            Require(target.TryGetGlobals(out reset), "Clear keeps globals registered");
            Equal(0, reset.Score, "Clear resets globals");
        }

        private static void TestCaptureAllocation()
        {
            CYGWorld world = new CYGWorld();
            world.SetGlobals(new TestGlobals { Score = 1 });
            world.CreateEntity();
            CYGWorldSnapshot snapshot = world.CreateSnapshot();

            long before = GC.GetAllocatedBytesForCurrentThread();
            for (int i = 0; i < 1000; i++)
            {
                world.GetGlobals<TestGlobals>().Score = i;
                world.Capture(snapshot);
            }
            long allocated = GC.GetAllocatedBytesForCurrentThread() - before;
            Equal(0L, allocated, "capture with globals allocates nothing after warm-up");
        }

        private sealed class CYGTestWorldFactory
        {
            public CYGWorld Create()
            {
                CYGWorld world = new CYGWorld();
                world.Register<TestPosition>();
                world.CreateEntity();
                return world;
            }
        }

        private struct OtherGlobals : ICYGValue
        {
            public int Value;

            public int WireSize
            {
                get { return 4; }
            }

            public int WriteTo(byte[] buffer, int offset)
            {
                CYGWire.WriteInt32(buffer, offset, Value);
                return 4;
            }

            public void ReadFrom(byte[] buffer, int offset)
            {
                Value = CYGWire.ReadInt32(buffer, offset);
            }

            public void ComputeChecksum(ref uint crc)
            {
                CYGChecksum.HashInt(ref crc, Value);
            }
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
                throw new InvalidOperationException(
                    message + " (expected " + expected + ", got " + actual + ")");
            }
        }
    }
}
