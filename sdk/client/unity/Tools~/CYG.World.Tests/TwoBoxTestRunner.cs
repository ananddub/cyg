using System;
using System.Collections.Generic;
using CookYourGame.Sim;
using CookYourGame.Sim.TwoBox;

namespace CookYourGame.Sim.Tests
{
    public static class TwoBoxTestRunner
    {
        private static int _assertions;

        public static int AssertionCount
        {
            get { return _assertions; }
        }

        public static void Run()
        {
            TestSchemaContract();
            TestSingleCollectionPerTick();
            TestLocalAuthorityRunsPastPredictionWindow();
            TestLateInputRejectionIsObservable();
            TestTwoClientMatch();
            TestLateRemoteInputConverges();
            Console.WriteLine($"[PASS] TwoBox determinism suite ({_assertions} assertions)");
        }

        private static void TestSchemaContract()
        {
            Equal(1010, BoxPlayer.TypeId, "BoxPlayer type id");
            Equal(6, BoxPlayer.SizeInBytes, "BoxPlayer wire size");
            Equal(0x75401846u, BoxPlayer.SchemaHash, "BoxPlayer schema hash");

            Equal(1011, BoxMovement.TypeId, "BoxMovement type id");
            Equal(32, BoxMovement.SizeInBytes, "BoxMovement wire size");
            Equal(0x8DE4372Fu, BoxMovement.SchemaHash, "BoxMovement schema hash");

            Equal(1012, GemTarget.TypeId, "GemTarget type id");
            Equal(33, GemTarget.SizeInBytes, "GemTarget wire size");
            Equal(0x3EA30839u, GemTarget.SchemaHash, "GemTarget schema hash");
        }

        private static void TestSingleCollectionPerTick()
        {
            CYGWorld world = new CYGWorld(7UL);
            RegisterTwoBox(world);

            CYGEntityRef p1 = SpawnPlayer(world, 1, new FPVector3(FP.Zero, FP.FromFraction(1, 2), FP.Zero));
            CYGEntityRef p2 = SpawnPlayer(world, 2, new FPVector3(FP.Zero, FP.FromFraction(1, 2), FP.Zero));
            CYGEntityRef gem = SpawnGem(world, FPVector3.Zero);

            CYGSystemRunner systems = CreateSystems(world, CYGDeterminismConfig.Default);
            world.Events.BeginTick(1);
            systems.Tick(1, false, null, null, 0);

            BoxPlayer first;
            BoxPlayer second;
            Require(world.TryGet(p1, out first), "p1 component");
            Require(world.TryGet(p2, out second), "p2 component");
            Equal(1, first.Score + second.Score, "only one player collects the gem in a tick");
            Equal(1, world.Events.PendingCount, "exactly one collection event");
            Require(world.TryGet(gem, out GemTarget target), "gem component");
            Require(target.IsActive, "gem stays active after respawn");
        }

        private static void TestLocalAuthorityRunsPastPredictionWindow()
        {
            CYGEntityRef p1;
            CYGEntityRef p2;
            CYGEntityRef gem;
            CYGWorldRollbackSession session = CreateSession(
                11UL,
                maxPredictionTicks: 2,
                rollbackCapacity: 16,
                autoConfirm: true,
                out p1,
                out p2,
                out gem);

            for (int tick = 0; tick < 100; tick++)
            {
                session.SetLocalInput(tick, CreatePlayerOneInput(tick));
                Require(session.Step(), "local-authority step " + tick);
            }

            Equal(100, session.Frame, "local authority frame");
            Equal(99, session.ConfirmedTick, "local authority confirms simulated ticks");
            Equal(1, session.PredictionLead, "local authority keeps a one-frame lead");
            Require(session.RejectedInputs == 0, "no rejected inputs");
        }

        private static void TestLateInputRejectionIsObservable()
        {
            CYGEntityRef p1;
            CYGEntityRef p2;
            CYGEntityRef gem;
            CYGWorldRollbackSession session = CreateSession(
                12UL,
                maxPredictionTicks: 4,
                rollbackCapacity: 8,
                autoConfirm: true,
                out p1,
                out p2,
                out gem);

            for (int tick = 0; tick < 20; tick++)
            {
                session.SetLocalInput(tick, CreatePlayerOneInput(tick));
                Require(session.Step(), "rejection test step");
            }

            CYGInputRejectionReason observed = CYGInputRejectionReason.None;
            session.OnInputRejected += (playerId, tick, reason) => observed = reason;

            Require(!session.OnRemoteInputReceived(2, new CYGPlayerInput(0, 0, 0, 0)), "old input is rejected");
            Equal(CYGInputRejectionReason.OlderThanRollbackWindow, observed, "old input rejection reason");

            Require(!session.OnRemoteInputReceived(2, new CYGPlayerInput(200, 0, 0, 0)), "future input is rejected");
            Equal(CYGInputRejectionReason.AheadOfPredictionWindow, observed, "future input rejection reason");
            Equal(2, session.RejectedInputs, "rejected input counter");
        }

        private static void TestTwoClientMatch()
        {
            CYGEntityRef firstP1;
            CYGEntityRef firstP2;
            CYGEntityRef firstGem;
            CYGWorldRollbackSession first = CreateSession(
                12345UL, maxPredictionTicks: 10, rollbackCapacity: 64, autoConfirm: true,
                out firstP1, out firstP2, out firstGem);

            CYGEntityRef secondP1;
            CYGEntityRef secondP2;
            CYGEntityRef secondGem;
            CYGWorldRollbackSession second = CreateSession(
                12345UL, maxPredictionTicks: 10, rollbackCapacity: 64, autoConfirm: true,
                out secondP1, out secondP2, out secondGem);

            const int ticks = 600;
            for (int tick = 0; tick < ticks; tick++)
            {
                CYGPlayerInput p1Input = CreatePlayerOneInput(tick);
                first.SetLocalInput(tick, p1Input);
                second.SetLocalInput(tick, p1Input);

                CYGPlayerInput firstBot = CreateBotInput(first.World, firstP2, firstGem, tick);
                CYGPlayerInput secondBot = CreateBotInput(second.World, secondP2, secondGem, tick);
                Require(firstBot.ActionsEqual(secondBot), "bot inputs agree at tick " + tick);

                first.SetPlayerInput(2, tick, firstBot, isPredicted: false);
                second.SetPlayerInput(2, tick, secondBot, isPredicted: false);

                Require(first.Step(), "first client step " + tick);
                Require(second.Step(), "second client step " + tick);

                if (tick % 60 == 0)
                {
                    Equal(first.Checksum, second.Checksum, "checksum at tick " + tick);
                }
            }

            Equal(ticks, first.Frame, "first client frame");
            Equal(ticks, second.Frame, "second client frame");
            Equal(first.Checksum, second.Checksum, "two client final checksum");
            Equal(first.ConfirmedTick, second.ConfirmedTick, "two client confirmed tick");

            BoxPlayer firstScoreA;
            BoxPlayer firstScoreB;
            BoxPlayer secondScoreA;
            BoxPlayer secondScoreB;
            Require(first.World.TryGet(firstP1, out firstScoreA), "first p1 score");
            Require(first.World.TryGet(firstP2, out firstScoreB), "first p2 score");
            Require(second.World.TryGet(secondP1, out secondScoreA), "second p1 score");
            Require(second.World.TryGet(secondP2, out secondScoreB), "second p2 score");
            Equal(firstScoreA.Score, secondScoreA.Score, "p1 score match");
            Equal(firstScoreB.Score, secondScoreB.Score, "p2 score match");

            List<CYGEvent> firstEvents = DispatchEvents(first);
            List<CYGEvent> secondEvents = DispatchEvents(second);
            Equal(CountGemEvents(firstEvents), CountGemEvents(secondEvents), "event count match");
        }

        private static void TestLateRemoteInputConverges()
        {
            const int ticks = 20;

            CYGEntityRef referenceP1;
            CYGEntityRef referenceP2;
            CYGEntityRef referenceGem;
            CYGWorldRollbackSession reference = CreateSession(
                99UL, maxPredictionTicks: 32, rollbackCapacity: 32, autoConfirm: true,
                out referenceP1, out referenceP2, out referenceGem);

            CYGEntityRef lateP1;
            CYGEntityRef lateP2;
            CYGEntityRef lateGem;
            CYGWorldRollbackSession late = CreateSession(
                99UL, maxPredictionTicks: 32, rollbackCapacity: 32, autoConfirm: true,
                out lateP1, out lateP2, out lateGem);

            CYGPlayerInput[] remoteInputs = new CYGPlayerInput[ticks];
            for (int tick = 0; tick < ticks; tick++)
            {
                CYGPlayerInput local = CreatePlayerOneInput(tick);
                reference.SetLocalInput(tick, local);
                late.SetLocalInput(tick, local);

                remoteInputs[tick] = CreateBotInput(reference.World, referenceP2, referenceGem, tick);
                reference.SetPlayerInput(2, tick, remoteInputs[tick], isPredicted: false);

                Require(reference.Step(), "reference step " + tick);
                Require(late.Step(), "late step " + tick);
            }

            Require(reference.Checksum != late.Checksum, "missing remote input diverges");

            for (int tick = 0; tick < ticks; tick++)
            {
                Require(late.OnRemoteInputReceived(2, remoteInputs[tick]), "late input rollback " + tick);
            }

            Equal(ticks, late.Frame, "late frame after correction");
            Equal(reference.Checksum, late.Checksum, "late client converges");
            Require(!late.IsResimulating, "resimulation cleared");

            List<CYGEvent> referenceEvents = DispatchEvents(reference);
            List<CYGEvent> lateEvents = DispatchEvents(late);
            Equal(referenceEvents.Count, lateEvents.Count, "corrected event count");
            for (int i = 0; i < referenceEvents.Count; i++)
            {
                GemCollectedEvent expected = referenceEvents[i] as GemCollectedEvent;
                GemCollectedEvent actual = lateEvents[i] as GemCollectedEvent;
                Require(expected != null && actual != null, "gem event type at index " + i);
                Equal(expected.Tick, actual.Tick, "event tick at index " + i);
                Equal(expected.Player.Value, actual.Player.Value, "event player at index " + i);
                Equal(expected.NewScore, actual.NewScore, "event score at index " + i);
            }
        }

        private static CYGWorldRollbackSession CreateSession(
            ulong seed,
            int maxPredictionTicks,
            int rollbackCapacity,
            bool autoConfirm,
            out CYGEntityRef p1,
            out CYGEntityRef p2,
            out CYGEntityRef gem)
        {
            CYGWorld world = new CYGWorld(seed);
            RegisterTwoBox(world);

            CYGDeterminismConfig config = new CYGDeterminismConfig(
                maxPlayers: 2,
                seed: seed,
                rollbackCapacity: rollbackCapacity,
                maxPredictionTicks: maxPredictionTicks);

            CYGWorldRollbackSession session = new CYGWorldRollbackSession(
                world,
                config,
                CYGInputRingBuffer.DefaultCapacity,
                2,
                defaultCharacterMovement: false);

            CYGSystemRunner systems = CreateSystems(world, config);
            session.SetSystems(systems);
            session.LocalPlayerId = 1;
            session.AutoConfirmLocalTicks = autoConfirm;

            p1 = SpawnPlayer(world, 1, new FPVector3(FP.FromInt(-3), FP.FromFraction(1, 2), FP.Zero));
            p2 = SpawnPlayer(world, 2, new FPVector3(FP.FromInt(3), FP.FromFraction(1, 2), FP.Zero));
            gem = SpawnGem(world, new FPVector3(FP.Zero, FP.FromFraction(3, 5), FP.Zero));
            return session;
        }

        private static CYGSystemRunner CreateSystems(CYGWorld world, CYGDeterminismConfig config)
        {
            CYGSystemRunner systems = new CYGSystemRunner(world, config);
            systems.AddSystem(new TwoBoxMovementSystem(), order: 10);
            systems.AddSystem(new GemCollisionSystem(), order: 20);
            systems.Start();
            return systems;
        }

        private static void RegisterTwoBox(CYGWorld world)
        {
            world.Register<CYGSimAgent>();
            world.Register<BoxPlayer>();
            world.Register<BoxMovement>();
            world.Register<GemTarget>();
        }

        private static CYGEntityRef SpawnPlayer(CYGWorld world, int playerId, FPVector3 position)
        {
            CYGEntityRef entity = world.CreateEntity();
            world.Set(entity, CYGSimAgent.Create(playerId, position));
            world.Set(entity, new BoxPlayer { Player = new CYGPlayerRef((ushort)playerId), Score = 0 });
            world.Set(entity, new BoxMovement { Speed = FP.FromInt(6) });
            return entity;
        }

        private static CYGEntityRef SpawnGem(CYGWorld world, FPVector3 position)
        {
            CYGEntityRef gem = world.CreateEntity();
            world.Set(gem, new GemTarget
            {
                Radius = FP.FromFraction(7, 20),
                Position = position,
                IsActive = true
            });
            return gem;
        }

        private static CYGPlayerInput CreatePlayerOneInput(int tick)
        {
            short moveX = (tick / 120) % 2 == 0 ? (short)1000 : (short)-1000;
            short moveY = (tick / 90) % 2 == 0 ? (short)700 : (short)-700;
            byte buttons = tick % 45 == 0 ? CYGPlayerInput.ButtonJump : (byte)0;
            return new CYGPlayerInput(tick, buttons, moveX, moveY);
        }

        private static CYGPlayerInput CreateBotInput(
            CYGWorld world,
            CYGEntityRef playerEntity,
            CYGEntityRef gemEntity,
            int tick)
        {
            short moveX = 0;
            short moveY = 0;
            byte buttons = 0;

            CYGSimAgent agent;
            GemTarget gem;
            if (world.TryGet(playerEntity, out agent) && world.TryGet(gemEntity, out gem))
            {
                FP deadzone = FP.FromFraction(1, 10);
                FP dx = gem.Position.x - agent.Position.x;
                FP dz = gem.Position.z - agent.Position.z;

                if (dx > deadzone) moveX = 1000;
                else if (dx < -deadzone) moveX = -1000;

                if (dz > deadzone) moveY = 1000;
                else if (dz < -deadzone) moveY = -1000;

                if (moveX != 0 && moveY != 0)
                {
                    moveX = moveX > 0 ? (short)707 : (short)-707;
                    moveY = moveY > 0 ? (short)707 : (short)-707;
                }
            }

            if (tick % 150 == 0)
            {
                buttons |= CYGPlayerInput.ButtonJump;
            }

            return new CYGPlayerInput(tick, buttons, moveX, moveY);
        }

        private static List<CYGEvent> DispatchEvents(CYGWorldRollbackSession session)
        {
            List<CYGEvent> events = new List<CYGEvent>();
            session.DispatchEvents(events, forceAll: true);
            return events;
        }

        private static int CountGemEvents(List<CYGEvent> events)
        {
            int count = 0;
            for (int i = 0; i < events.Count; i++)
            {
                if (events[i] is GemCollectedEvent) count++;
            }
            return count;
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
