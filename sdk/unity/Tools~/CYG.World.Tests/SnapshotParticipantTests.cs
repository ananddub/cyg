using System;

namespace CookYourGame.Sim.Tests
{
    internal static class SnapshotParticipantTests
    {
        public static void RunAll()
        {
            ChecksumCoversParticipant();
            ParticipantOrderIsStable();
            RollbackRestoresParticipant();
            FingerprintIncludesParticipants();
            SessionChecksumIncludesParticipant();
            LateRegistrationIsRejected();
            DuplicateIdIsRejected();
            ShortSnapshotRingIsRejected();
        }

        private static void ChecksumCoversParticipant()
        {
            CounterParticipant left = new CounterParticipant(7, 64);
            CounterParticipant right = new CounterParticipant(7, 64);
            CYGWorldRunner leftRunner = BuildRunner(left);
            CYGWorldRunner rightRunner = BuildRunner(right);
            Action<CYGWorld, int> step = (world, tick) =>
            {
                left.Advance(1);
                right.Advance(1);
            };

            for (int i = 0; i < 20; i++)
            {
                leftRunner.Tick(step);
                rightRunner.Tick(step);
            }

            Require(leftRunner.Checksum == rightRunner.Checksum, "matching participants must produce matching checksums");
            uint before = leftRunner.Checksum;
            left.Advance(1);
            Require(leftRunner.Checksum != before, "participant state must change the runner checksum");
            Require(leftRunner.Checksum != rightRunner.Checksum, "participant state must distinguish the two runners");
        }

        private static void ParticipantOrderIsStable()
        {
            CounterParticipant first = new CounterParticipant(1, 64);
            CounterParticipant second = new CounterParticipant(2, 64);
            CYGWorldRunner ascending = BuildRunner(second);
            ascending.RegisterParticipant(first);
            CYGWorldRunner descending = BuildRunner(first);
            descending.RegisterParticipant(second);

            Action<CYGWorld, int> step = (world, tick) =>
            {
                first.Advance(1);
                second.Advance(3);
            };

            for (int i = 0; i < 10; i++)
            {
                ascending.Tick(step);
                descending.Tick(step);
            }

            Require(ascending.Checksum == descending.Checksum, "registration order must not change the checksum");
        }

        private static void RollbackRestoresParticipant()
        {
            CounterParticipant participant = new CounterParticipant(3, 64);
            CYGWorldRunner runner = BuildRunner(participant);
            Action<CYGWorld, int> step = (world, tick) => participant.Advance(1);

            for (int i = 0; i < 24; i++)
            {
                runner.Tick(step);
            }
            uint expected = runner.Checksum;
            Require(participant.Value == 24, "participant must advance once per tick, was " + participant.Value);

            Require(runner.Rollback(6), "rollback within the ring must succeed");
            Require(runner.Frame == 18, "rollback must land on frame 18, was " + runner.Frame);
            Require(participant.Value == 18, "rollback must restore participant state, was " + participant.Value);

            for (int i = 0; i < 6; i++)
            {
                runner.Tick(step);
            }
            Require(participant.Value == 24, "resimulation must rebuild participant state");
            Require(runner.Checksum == expected, "resimulated checksum must match the original");
        }

        private static void FingerprintIncludesParticipants()
        {
            CYGWorldRunner bare = BuildRunner();
            CYGWorldRunner withParticipant = BuildRunner(new CounterParticipant(11, 64));
            Require(bare.ComputeFingerprint() != withParticipant.ComputeFingerprint(),
                "participant ids must change the session fingerprint");
        }

        private static void SessionChecksumIncludesParticipant()
        {
            CYGWorld world = new CYGWorld();
            CounterParticipant participant = new CounterParticipant(13, 64);
            CYGWorldRollbackSession session = new CYGWorldRollbackSession(
                world,
                CreateConfig(),
                defaultCharacterMovement: false);
            session.RegisterParticipant(participant);
            session.AutoConfirmLocalTicks = true;
            session.OnSimulateStep += (w, tick, inputs, ids, count, dt) => participant.Advance(1);

            Require(session.Step(), "session step");
            uint first = session.Checksum;
            participant.Advance(4);
            Require(session.Checksum != first, "session checksum must include participant state");
            Require(session.Frame == 1, "session frame after one step");
        }

        private static void LateRegistrationIsRejected()
        {
            CYGWorldRunner runner = BuildRunner();
            runner.Tick((world, tick) => { });
            Expect<InvalidOperationException>(
                () => runner.RegisterParticipant(new CounterParticipant(5, 64)),
                "late participant registration");
        }

        private static void DuplicateIdIsRejected()
        {
            CYGWorldRunner runner = BuildRunner(new CounterParticipant(9, 64));
            Expect<InvalidOperationException>(
                () => runner.RegisterParticipant(new CounterParticipant(9, 64)),
                "duplicate participant id");
        }

        private static void ShortSnapshotRingIsRejected()
        {
            CYGWorldRunner runner = BuildRunner();
            Expect<ArgumentOutOfRangeException>(
                () => runner.RegisterParticipant(new CounterParticipant(10, 4)),
                "participant ring shorter than the rollback ring");
        }

        private static CYGWorldRunner BuildRunner(params ICYGSnapshotParticipant[] participants)
        {
            CYGWorld world = new CYGWorld();
            world.Register<TestPosition>();
            CYGWorldRunner runner = new CYGWorldRunner(world, CreateConfig());
            for (int i = 0; i < participants.Length; i++)
            {
                runner.RegisterParticipant(participants[i]);
            }
            return runner;
        }

        private static CYGDeterminismConfig CreateConfig()
        {
            return new CYGDeterminismConfig(
                tickRate: 60,
                maxPlayers: 4,
                seed: 0x51EEDUL,
                checksumInterval: 10,
                rollbackCapacity: 64,
                maxPredictionTicks: 8);
        }

        private static void Expect<TException>(Action action, string label)
            where TException : Exception
        {
            try
            {
                action();
            }
            catch (TException)
            {
                return;
            }
            throw new InvalidOperationException(label + " must throw " + typeof(TException).Name);
        }

        private static void Require(bool condition, string message)
        {
            if (!condition)
            {
                throw new InvalidOperationException(message);
            }
        }

        private sealed class CounterParticipant : ICYGSnapshotParticipant
        {
            private readonly int[] _ticks;
            private readonly long[] _values;

            public CounterParticipant(uint id, int capacity)
            {
                ParticipantId = id;
                _ticks = new int[capacity];
                _values = new long[capacity];
                for (int i = 0; i < _ticks.Length; i++)
                {
                    _ticks[i] = -1;
                }
            }

            public uint ParticipantId { get; }

            public string ParticipantName
            {
                get { return "counter-" + ParticipantId; }
            }

            public int SnapshotCapacity
            {
                get { return _ticks.Length; }
            }

            public long Value;

            public void Advance(long delta)
            {
                Value += delta;
            }

            public void CaptureSnapshot(int tick)
            {
                int slot = tick % _ticks.Length;
                _ticks[slot] = tick;
                _values[slot] = Value;
            }

            public void RestoreSnapshot(int tick)
            {
                int slot = tick % _ticks.Length;
                if (_ticks[slot] != tick)
                {
                    throw new InvalidOperationException(
                        ParticipantName + " has no snapshot for tick " + tick + ".");
                }
                Value = _values[slot];
            }

            public void DiscardSnapshotsAfter(int tick)
            {
                for (int i = 0; i < _ticks.Length; i++)
                {
                    if (_ticks[i] > tick)
                    {
                        _ticks[i] = -1;
                        _values[i] = 0;
                    }
                }
            }

            public uint ComputeChecksum()
            {
                uint crc = 0xFFFFFFFFu;
                CYGChecksum.HashLong(ref crc, Value);
                return crc ^ 0xFFFFFFFFu;
            }
        }
    }
}
