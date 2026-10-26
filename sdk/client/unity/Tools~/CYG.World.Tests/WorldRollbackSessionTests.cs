using System;

namespace CookYourGame.Sim.Tests
{
    internal static class WorldRollbackSessionTests
    {
        private static int _assertions;

        public static int AssertionCount
        {
            get { return _assertions; }
        }

        public static void RunAll()
        {
            TestDefaultMovementDeterminism();
            TestLateRemoteInputConverges();
            TestPredictionLimit();
            TestRollbackRestoresGlobals();
            TestZeroAllocationStepping();
        }

        private static void TestDefaultMovementDeterminism()
        {
            CYGWorldRollbackSession first = CreateSession(out CYGEntityRef firstPlayer);
            CYGWorldRollbackSession second = CreateSession(out CYGEntityRef secondPlayer);

            for (int tick = 0; tick < 120; tick++)
            {
                CYGPlayerInput input = CreateLocalInput(tick);
                first.SetLocalInput(tick, input);
                second.SetLocalInput(tick, input);
                Require(first.Step(), "first session step");
                Require(second.Step(), "second session step");
            }

            Equal(120, first.Frame, "first session frame");
            Equal(120, second.Frame, "second session frame");
            Equal(first.Checksum, second.Checksum, "identical sessions must checksum equal");
            Equal(0, first.TotalRollbacks, "no rollback without remote input");

            CYGSimAgent agent;
            Require(first.World.TryGet(firstPlayer, out agent), "default movement keeps the agent component");
            Require(second.World.TryGet(secondPlayer, out agent), "second session agent present");
        }

        private static void TestLateRemoteInputConverges()
        {
            CYGDeterminismConfig config = new CYGDeterminismConfig(
                maxPlayers: 2,
                rollbackCapacity: 32,
                maxPredictionTicks: 8);

            CYGWorldRollbackSession reference = CreateTwoPlayerSession(config, out _);
            CYGWorldRollbackSession late = CreateTwoPlayerSession(config, out _);

            const int ticks = 8;
            for (int tick = 0; tick < ticks; tick++)
            {
                CYGPlayerInput local = CreateLocalInput(tick);
                reference.SetLocalInput(tick, local);
                late.SetLocalInput(tick, local);
                CYGPlayerInput remote = CreateRemoteInput(tick);
                reference.OnRemoteInputReceived(2, remote);
            }

            for (int tick = 0; tick < ticks; tick++)
            {
                Require(reference.Step(), "reference step");
                Require(late.Step(), "late step");
            }

            Require(reference.Checksum != late.Checksum, "missing remote input must diverge before correction");

            for (int tick = 0; tick < ticks; tick++)
            {
                Require(late.OnRemoteInputReceived(2, CreateRemoteInput(tick)), "late input must roll back");
            }

            Equal(ticks, late.Frame, "late session frame after correction");
            Equal(reference.Checksum, late.Checksum, "late session must converge to reference checksum");
            Equal(ticks, late.TotalRollbacks, "one rollback per late input");
            Equal(36, late.TotalResimulatedTicks, "triangular resimulation count");
            Require(!late.IsResimulating, "resimulation flag cleared");

            for (int frame = 1; frame <= ticks; frame++)
            {
                uint expected;
                uint actual;
                Require(reference.VerifiedFrames.TryGetLocalChecksum(frame, out expected), "reference frame checksum");
                Require(late.VerifiedFrames.TryGetLocalChecksum(frame, out actual), "late frame checksum");
                Equal(expected, actual, "corrected frame checksum " + frame);
                Require(late.ConfirmFrame(frame, expected), "corrected frame verifies without desync");
            }

            Require(!late.VerifiedFrames.HasDesync, "no desync after correction");
            Equal(0, late.VerifiedFrames.MismatchCount, "no checksum mismatches");
        }

        private static void TestPredictionLimit()
        {
            CYGDeterminismConfig config = new CYGDeterminismConfig(
                maxPlayers: 2,
                rollbackCapacity: 32,
                maxPredictionTicks: 2);

            CYGWorldRollbackSession session = CreateTwoPlayerSession(config, out _);
            session.OnRemoteInputReceived(2, CreateRemoteInput(0));

            for (int tick = 0; tick < 4; tick++)
            {
                session.SetLocalInput(tick, CreateLocalInput(tick));
            }

            Require(session.Step(), "first predicted step");
            Require(session.Step(), "second predicted step");
            Require(session.Step(), "third predicted step");
            Require(!session.CanStepSimulation(), "prediction budget exhausted");
            Equal(3, session.Frame, "frame stops at prediction budget");
            Require(!session.Step(), "step is rejected past the prediction budget");
            Equal(3, session.Frame, "rejected step must not advance");
        }

        private static void TestRollbackRestoresGlobals()
        {
            CYGDeterminismConfig config = new CYGDeterminismConfig(
                maxPlayers: 2,
                rollbackCapacity: 32,
                maxPredictionTicks: 8);

            CYGWorldRollbackSession session = CreateTwoPlayerSession(config, out _);
            session.World.SetGlobals(new TestGlobals { Score = 5, Timer = FP.FromInt(1) });

            for (int tick = 0; tick < 4; tick++)
            {
                session.SetLocalInput(tick, CreateLocalInput(tick));
                Require(session.Step(), "global test step");
            }

            session.World.GetGlobals<TestGlobals>().Score = 99;
            Require(session.OnRemoteInputReceived(2, CreateRemoteInput(1)), "past input triggers rollback");

            TestGlobals globals;
            Require(session.World.TryGetGlobals(out globals), "globals remain hosted");
            Equal(5, globals.Score, "rollback restores the global score");
            Equal(FP.FromInt(1), globals.Timer, "rollback restores the global timer");
        }

        private static void TestZeroAllocationStepping()
        {
            CYGWorldRollbackSession session = CreateSession(out _);

            for (int tick = 0; tick < 8; tick++)
            {
                session.SetLocalInput(tick, CreateLocalInput(tick));
                session.Step();
            }

            long before = GC.GetAllocatedBytesForCurrentThread();
            for (int i = 0; i < 120; i++)
            {
                int tick = session.Frame;
                session.SetLocalInput(tick, CreateLocalInput(tick));
                session.Step();
            }
            long allocated = GC.GetAllocatedBytesForCurrentThread() - before;
            Equal(0L, allocated, "ECS rollback stepping allocates nothing after warm-up");
        }

        private static CYGWorldRollbackSession CreateSession(out CYGEntityRef player)
        {
            CYGWorld world = new CYGWorld();
            CYGWorldRollbackSession session = new CYGWorldRollbackSession(world);
            player = world.CreateEntity();
            world.Set(player, CYGSimAgent.Create(1, new FPVector3(FP.Zero, FP.Zero, FP.Zero)));
            session.LocalPlayerId = 1;
            return session;
        }

        private static CYGWorldRollbackSession CreateTwoPlayerSession(
            CYGDeterminismConfig config,
            out CYGEntityRef first)
        {
            CYGWorld world = new CYGWorld();
            CYGWorldRollbackSession session = new CYGWorldRollbackSession(world, config);
            first = world.CreateEntity();
            world.Set(first, CYGSimAgent.Create(1, new FPVector3(FP.Zero, FP.Zero, FP.Zero)));
            CYGEntityRef second = world.CreateEntity();
            world.Set(second, CYGSimAgent.Create(2, new FPVector3(FP.FromInt(2), FP.Zero, FP.Zero)));
            session.LocalPlayerId = 1;
            return session;
        }

        private static CYGPlayerInput CreateLocalInput(int tick)
        {
            return new CYGPlayerInput(tick, (byte)(tick % 17 == 0 ? CYGPlayerInput.ButtonJump : 0), 1000, 0);
        }

        private static CYGPlayerInput CreateRemoteInput(int tick)
        {
            return new CYGPlayerInput(
                tick,
                0,
                (short)(300 + tick * 10),
                (short)(-200 - tick * 5));
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
