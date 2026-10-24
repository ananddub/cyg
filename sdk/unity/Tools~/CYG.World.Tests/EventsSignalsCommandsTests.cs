using System;
using System.Collections.Generic;

namespace CookYourGame.Sim.Tests
{
    internal static class EventsSignalsCommandsTests
    {
        public static int AssertionCount
        {
            get { return _assertions; }
        }

        public static void RunAll()
        {
            TestSignalBus();
            TestSignalsTickAndRollback();
            TestSyncedEventVerification();
            TestLocalEventDispatch();
            TestEventRollbackExactlyOnce();
            TestVerifiedEventRollback();
            TestEventCancel();
            TestCommandLookupAndRepeat();
            TestCommandVerification();
            TestCommandRingWrap();
            TestCommandPacketRoundtrip();
            TestCommandPacketValidation();
            TestWorldClearClearsChannels();
            TestRunnerSystemLoop();
            TestOwnerScopedEventSubscriptions();
        }

        private static void TestOwnerScopedEventSubscriptions()
        {
            CYGEvents.Clear();
            TestEvent evt = new TestEvent(9001, CYGEventRoute.Local);
            object ownerA = new object();
            object ownerB = new object();
            int countA = 0;
            int countB = 0;

            CYGEventSubscription<TestEvent> subscriptionA = (CYGEventSubscription<TestEvent>)
                CYGEvents.Subscribe(ownerA, (TestEvent e) => countA++);
            CYGEvents.Subscribe(ownerB, (TestEvent e) => countB++);

            CYGEvents.Dispatch(evt);
            Equal(1, countA, "owner A receives event");
            Equal(1, countB, "owner B receives event");

            CYGEvents.UnsubscribeAll(ownerA);
            Require(subscriptionA.IsDisposed, "owner subscription reports disposed");
            CYGEvents.Dispatch(evt);
            Equal(1, countA, "disposed owner handler must not run");
            Equal(2, countB, "other owner still receives events");

            CYGEvents.UnsubscribeAll(ownerB);
            CYGEvents.Dispatch(evt);
            Equal(2, countB, "second owner unsubscribed");

            Require(Throws<ArgumentNullException>(
                () => CYGEvents.Subscribe<TestEvent>(ownerA, null)),
                "null handler must throw");

            subscriptionA.Dispose();
            Require(subscriptionA.IsDisposed, "double dispose is safe");
            CYGEvents.Clear();
        }

        private static void TestSignalBus()
        {
            CYGSignalBus signals = new CYGSignalBus();
            Require(!signals.Any, "signal bus starts empty");
            signals.Raise(3);
            signals.Set(7, true);
            Require(signals.IsSet(3), "raised signal");
            Require(signals.IsSet(7), "set signal");
            Require(signals.Has((1u << 3) | (1u << 7)), "Has all bits");
            Require(signals.HasAny((1u << 7) | (1u << 9)), "HasAny one bit");
            Require(!signals.HasAny(1u << 9), "HasAny missing bit");
            signals.Clear(1u << 3);
            Require(!signals.IsSet(3), "cleared signal");
            signals.Set(7, false);
            Require(!signals.Any, "clearing every signal empties the mask");
            signals.Raise(31);
            Require(signals.IsSet(31), "last signal index");

            bool threw = false;
            try
            {
                signals.Raise(32);
            }
            catch (ArgumentOutOfRangeException)
            {
                threw = true;
            }
            Require(threw, "signal index 32 must throw");
        }

        private static void TestSignalsTickAndRollback()
        {
            CYGWorld world = new CYGWorld();
            CYGWorldRunner runner = new CYGWorldRunner(world, 8);

            runner.Tick(w => w.Signals.Raise(1));
            Require(world.Signals.IsSet(1), "signal remains readable after the tick");
            runner.Tick(w => Require(!w.Signals.IsSet(1), "runner must clear signals at tick start"));
            Require(!world.Signals.Any, "signals stay clear after the tick");

            runner.Tick(w => w.Signals.Raise(2));
            Require(runner.Rollback(1), "rollback after signal tick");
            Require(!world.Signals.Any, "rollback clears tick-local signals");
        }

        private static void TestSyncedEventVerification()
        {
            CYGWorld world = new CYGWorld();
            CYGWorldRunner runner = new CYGWorldRunner(world, 8);
            List<CYGEvent> view = new List<CYGEvent>();

            runner.Tick((w, tick) => w.Events.Publish(new TestEvent(100, CYGEventRoute.Synced)));
            Equal(0, runner.DispatchEvents(view), "verified event must wait for confirmation");

            runner.ConfirmUpTo(0);
            Equal(1, runner.DispatchEvents(view), "confirmed event dispatches");
            Equal(0, ((TestEvent)view[0]).Tick, "event carries its simulation tick");
            Equal(100, view[0].TypeId, "event type id");

            Equal(0, runner.DispatchEvents(view), "dispatched events are not repeated");
            runner.ConfirmUpTo(10);
            Equal(0, runner.DispatchEvents(view), "confirming beyond the tick must not resurrect events");
        }

        private static void TestLocalEventDispatch()
        {
            CYGWorld world = new CYGWorld();
            CYGWorldRunner runner = new CYGWorldRunner(world, 8);
            List<CYGEvent> view = new List<CYGEvent>();

            runner.Tick(w => w.Events.Publish(new TestEvent(101, CYGEventRoute.Local)));
            Equal(1, runner.DispatchEvents(view), "local event dispatches without confirmation");
            Equal(0, world.Events.PendingCount, "dispatched local event is no longer pending");
        }

        private static void TestEventRollbackExactlyOnce()
        {
            CYGWorld world = new CYGWorld();
            CYGWorldRunner runner = new CYGWorldRunner(world, 8);
            List<CYGEvent> view = new List<CYGEvent>();

            runner.Tick(w => w.Events.Publish(new TestEvent(102, CYGEventRoute.Local)));
            Equal(1, runner.DispatchEvents(view), "first local event");
            runner.Tick(w => w.Events.Publish(new TestEvent(103, CYGEventRoute.Local)));
            Equal(1, runner.DispatchEvents(view), "second local event");

            Require(runner.Rollback(1), "rollback one frame");
            Equal(0, runner.DispatchEvents(view), "rollback discards the predicted tick");

            runner.Tick(w => w.Events.Publish(new TestEvent(103, CYGEventRoute.Local)));
            Equal(0, runner.DispatchEvents(view), "re-emitted predicted event must stay exactly-once");
            Equal(2, view.Count, "view event count after resimulation");
        }

        private static void TestVerifiedEventRollback()
        {
            CYGWorld world = new CYGWorld();
            CYGWorldRunner runner = new CYGWorldRunner(world, 8);
            List<CYGEvent> view = new List<CYGEvent>();

            runner.Tick(w => w.Events.Publish(new TestEvent(104, CYGEventRoute.Synced)));
            runner.Tick(w => w.Events.Publish(new TestEvent(105, CYGEventRoute.Synced)));

            Require(runner.Rollback(1), "rollback verified event tick");
            runner.ConfirmUpTo(1);
            Equal(1, runner.DispatchEvents(view), "only the surviving verified event dispatches");
            Equal(104, view[0].TypeId, "surviving event type");

            runner.Tick((w, tick) => w.Events.Publish(new TestEvent(105, CYGEventRoute.Synced)));
            runner.ConfirmUpTo(2);
            Equal(1, runner.DispatchEvents(view), "re-simulated verified event dispatches once");
            Equal(105, view[1].TypeId, "re-simulated event type");
        }

        private static void TestEventCancel()
        {
            CYGEventBuffer buffer = new CYGEventBuffer();
            buffer.BeginTick(5);
            buffer.Publish(new TestEvent(106, CYGEventRoute.Local));
            buffer.CancelTick(5);

            List<CYGEvent> view = new List<CYGEvent>();
            Equal(0, buffer.DispatchReady(view), "canceled tick must not dispatch");
            buffer.BeginTick(6);
            buffer.Publish(new TestEvent(107, CYGEventRoute.Local));
            Equal(1, buffer.DispatchReady(view), "next tick dispatches normally");
        }

        private static void TestCommandLookupAndRepeat()
        {
            CYGCommandBuffer commands = new CYGCommandBuffer(maxPlayers: 2, capacity: 8, repeatLookback: 4);

            commands.AddCommand(0, 10, new TestCommand(9001) { Value = 7 });
            CYGCommand command;
            Require(commands.TryGetCommand(0, 10, out command), "exact command lookup");
            Equal(0, command.PlayerId, "command player");
            Equal(10, command.Tick, "command tick");
            Equal(7, ((TestCommand)command).Value, "command payload");

            Require(commands.TryGetExecutableCommand(0, 10, false, out command), "exact command is executable");
            Require(!commands.TryGetExecutableCommand(0, 11, false, out command), "no repeat without the flag");

            commands.AddCommand(0, 11, new RepeatCommand { Value = 3 });
            Require(commands.TryGetExecutableCommand(0, 13, false, out command), "repeated command is found");
            Equal(11, command.Tick, "repeat keeps its original tick");

            commands.AddCommand(0, 12, new TestCommand(9001) { Value = 9 });
            Require(!commands.TryGetExecutableCommand(0, 13, false, out command),
                "a non-repeat command ends the repeat chain");

            commands.AddCommand(1, 12, new TestCommand(9001) { Value = 1, FlagsOverride = CYGCommandFlags.Disabled });
            Require(!commands.TryGetExecutableCommand(1, 12, false, out command), "disabled command must not execute");

            CYGCommand ignored = new TestCommand(9001) { Value = 2, FlagsOverride = CYGCommandFlags.IgnoreOnResimulate };
            commands.AddCommand(1, 14, ignored);
            Require(commands.TryGetExecutableCommand(1, 14, false, out command), "ignored command executes live");
            Require(!commands.TryGetExecutableCommand(1, 14, true, out command), "ignored command skipped on resim");
        }

        private static void TestCommandVerification()
        {
            CYGCommandBuffer commands = new CYGCommandBuffer(maxPlayers: 1, capacity: 8);
            commands.AddCommand(0, 0, new TestCommand(9001));
            commands.AddCommand(0, 1, new TestCommand(9001));
            commands.AddCommand(0, 2, new TestCommand(9001));

            Equal(-1, commands.VerifiedTick(0), "no verified tick initially");
            Require(!commands.IsVerified(0, 0), "predicted command is not verified");

            commands.MarkVerified(0, 1);
            Equal(1, commands.VerifiedTick(0), "verified tick");
            Require(commands.IsVerified(0, 0), "older command verified");
            Require(commands.IsVerified(0, 1), "tick command verified");
            Require(!commands.IsVerified(0, 2), "future command stays predicted");

            commands.MarkVerified(0, 0);
            Equal(1, commands.VerifiedTick(0), "verified tick never regresses");
        }

        private static void TestCommandRingWrap()
        {
            CYGCommandBuffer commands = new CYGCommandBuffer(maxPlayers: 1, capacity: 4);
            for (int tick = 0; tick < 4; tick++)
            {
                commands.AddCommand(0, tick, new TestCommand(9001) { Value = tick });
            }

            CYGCommand command;
            Require(commands.TryGetCommand(0, 0, out command), "tick 0 before wrap");
            commands.AddCommand(0, 4, new TestCommand(9001) { Value = 4 });
            Require(!commands.TryGetCommand(0, 0, out command), "tick 0 is overwritten after wrap");
            Require(commands.TryGetCommand(0, 4, out command), "newest tick after wrap");
            Equal(4, ((TestCommand)command).Value, "wrapped command payload");
        }

        private static void TestCommandPacketRoundtrip()
        {
            CYGCommandBuffer sender = new CYGCommandBuffer(maxPlayers: 2, capacity: 8);
            sender.RegisterCommand(9001, () => new TestCommand(9001));
            sender.AddCommand(1, 3, new TestCommand(9001) { Value = 77 });
            sender.AddCommand(1, 5, new TestCommand(9001) { Value = 88 });

            byte[] buffer = new byte[256];
            int written = sender.WritePacket(1, 3, 4, buffer, 0);
            Require(written > CYGCommandBuffer.PacketHeaderSize, "packet contains entries");

            CYGCommandBuffer receiver = new CYGCommandBuffer(maxPlayers: 2, capacity: 8);
            receiver.RegisterCommand(9001, () => new TestCommand(9001));
            int player;
            int firstTick;
            Equal(2, receiver.ReadPacket(buffer, 0, written, out player, out firstTick), "packet entry count");
            Equal(1, player, "packet player");
            Equal(3, firstTick, "packet first tick");

            CYGCommand command;
            Require(receiver.TryGetCommand(1, 3, out command), "tick 3 arrived");
            Equal(77, ((TestCommand)command).Value, "tick 3 payload");
            Require(receiver.TryGetCommand(1, 5, out command), "tick 5 arrived");
            Equal(88, ((TestCommand)command).Value, "tick 5 payload");
            Require(!receiver.TryGetCommand(1, 4, out command), "empty ticks stay empty");
        }

        private static void TestCommandPacketValidation()
        {
            CYGCommandBuffer commands = new CYGCommandBuffer(maxPlayers: 2, capacity: 4);
            commands.RegisterCommand(9001, () => new TestCommand(9001));
            commands.AddCommand(0, 0, new TestCommand(9001) { Value = 5 });
            byte[] buffer = new byte[128];
            int written = commands.WritePacket(0, 0, 2, buffer, 0);

            CYGCommandBuffer unregistered = new CYGCommandBuffer(maxPlayers: 2, capacity: 4);
            Require(Throws<InvalidOperationException>(() =>
            {
                int player;
                int firstTick;
                unregistered.ReadPacket(buffer, 0, written, out player, out firstTick);
            }), "unknown command type must fail to decode");

            Require(Throws<InvalidOperationException>(() =>
            {
                int player;
                int firstTick;
                commands.ReadPacket(buffer, 0, written - 1, out player, out firstTick);
            }), "truncated packet must fail");

            Require(Throws<ArgumentOutOfRangeException>(() =>
                commands.WritePacket(0, 0, 99, buffer, 0)), "tick count beyond capacity must fail");
            Require(Throws<ArgumentOutOfRangeException>(() =>
                commands.AddCommand(2, 0, new TestCommand(9001))), "player beyond max must fail");
        }

        private static void TestWorldClearClearsChannels()
        {
            CYGWorld world = new CYGWorld();
            world.Events.BeginTick(0);
            world.Events.Publish(new TestEvent(108, CYGEventRoute.Local));
            world.Signals.Raise(1);
            world.Commands.AddCommand(0, 0, new TestCommand(9001));

            world.Clear();

            Equal(0, world.Events.PendingCount, "Clear empties events");
            Require(!world.Signals.Any, "Clear resets signals");
            CYGCommand command;
            Require(!world.Commands.TryGetCommand(0, 0, out command), "Clear empties commands");
        }

        private static void TestRunnerSystemLoop()
        {
            CYGWorld world = new CYGWorld();
            CYGSystemRunner systems = new CYGSystemRunner(world, CYGDeterminismConfig.Default);
            SignalSystem system = new SignalSystem();
            systems.AddSystem(system, order: 1);
            CYGWorldRunner runner = new CYGWorldRunner(world, 8);
            List<CYGEvent> view = new List<CYGEvent>();

            runner.Tick(systems);
            Equal(0, system.LastTick, "system receives the runner tick");
            Equal(1, system.Updates, "system ran");
            Equal(1, runner.DispatchEvents(view), "system local event dispatched");

            runner.Tick(systems);
            Require(system.SawClearedSignal, "runner clears signals before systems run");
            Equal(1, system.LastTick, "tick advances");
        }

        private static bool Throws<TException>(Action action) where TException : Exception
        {
            try
            {
                action();
            }
            catch (TException)
            {
                return true;
            }
            return false;
        }

        private static int _assertions;

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

        private sealed class TestEvent : CYGEvent
        {
            private readonly int _typeId;
            private readonly CYGEventRoute _route;

            public TestEvent(int typeId, CYGEventRoute route)
            {
                _typeId = typeId;
                _route = route;
            }

            public override int TypeId
            {
                get { return _typeId; }
            }

            public override CYGEventRoute Route
            {
                get { return _route; }
            }
        }

        private class TestCommand : CYGCommand
        {
            private readonly int _typeId;

            public TestCommand(int typeId)
            {
                _typeId = typeId;
            }

            public override int TypeId
            {
                get { return _typeId; }
            }

            public CYGCommandFlags FlagsOverride { get; set; }

            public override CYGCommandFlags Flags
            {
                get { return FlagsOverride; }
            }

            public int Value;

            public override int PayloadSize
            {
                get { return 4; }
            }

            public override int WritePayload(byte[] buffer, int offset)
            {
                CYGWire.WriteInt32(buffer, offset, Value);
                return 4;
            }

            public override void ReadPayload(byte[] buffer, int offset, int length)
            {
                Value = CYGWire.ReadInt32(buffer, offset);
            }
        }

        private sealed class RepeatCommand : TestCommand
        {
            public RepeatCommand()
                : base(9002)
            {
            }

            public override CYGCommandFlags Flags
            {
                get { return CYGCommandFlags.Repeat; }
            }

            public override int PayloadSize
            {
                get { return 4; }
            }
        }

        private sealed class SignalSystem : CYGSystem
        {
            public int Updates;
            public int LastTick;
            public bool SawClearedSignal;

            public override void OnUpdate(in CYGSystemContext context)
            {
                Updates++;
                LastTick = context.Tick;
                if (context.Tick > 0)
                {
                    SawClearedSignal = !context.World.Signals.IsSet(4);
                }
                context.World.Signals.Raise(4);
                context.World.Events.Publish(new TestEvent(109, CYGEventRoute.Local));
            }
        }
    }
}
