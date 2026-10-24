using System;
using System.Collections.Generic;

namespace CookYourGame.Sim.Tests
{
    internal static class NetcodeTests
    {
        public static int AssertionCount
        {
            get { return _assertions; }
        }

        public static void RunAll()
        {
            TestLatencyAndDelivery();
            TestLossAndDuplication();
            TestJitterBoundsAndDeterminism();
            TestPayloadIsolationAndOrdering();
            TestInputRedundancyRoundtrip();
            TestInputRedundancyValidation();
            TestVerifiedFrameTracker();
            TestRunnerNetcodeIntegration();
        }

        private static void TestLatencyAndDelivery()
        {
            CYGNetSimConfig config = new CYGNetSimConfig { LatencyTicks = 3 };
            CYGNetworkSimulator net = new CYGNetworkSimulator(config, 7UL);
            byte[] payload = { 1, 2, 3 };
            net.Send(0, 1, payload, 0, payload.Length);

            List<CYGNetPacket> delivered = new List<CYGNetPacket>();
            net.Advance();
            Equal(0, net.Receive(1, delivered), "packet is in flight");
            net.Advance();
            Equal(0, net.Receive(1, delivered), "packet is still in flight");
            net.Advance();
            Equal(1, net.Receive(1, delivered), "packet arrives after latency ticks");
            Equal(0, delivered[0].From, "sender");
            Equal(1, delivered[0].To, "receiver");
            Equal(3, delivered[0].DeliveryTick, "delivery tick");
            Equal(1, net.SentCount, "sent counter");
            Equal(1, net.DeliveredCount, "delivered counter");
            Equal(0, net.InFlightCount, "in-flight drained");

            CYGNetworkSimulator zeroLatency = new CYGNetworkSimulator(new CYGNetSimConfig { LatencyTicks = 0 });
            zeroLatency.Send(0, 1, payload, 0, payload.Length);
            delivered.Clear();
            Equal(0, zeroLatency.Receive(1, delivered), "zero latency still needs one advance");
            zeroLatency.Advance();
            Equal(1, zeroLatency.Receive(1, delivered), "zero-latency packet arrives on the next tick");
        }

        private static void TestLossAndDuplication()
        {
            CYGNetSimConfig lossy = new CYGNetSimConfig { LatencyTicks = 1, LossPerMille = 1000 };
            CYGNetworkSimulator net = new CYGNetworkSimulator(lossy, 11UL);
            byte[] payload = { 9 };
            for (int i = 0; i < 10; i++)
            {
                net.Send(0, 1, payload, 0, payload.Length);
            }
            net.Advance();
            net.Advance();
            List<CYGNetPacket> delivered = new List<CYGNetPacket>();
            Equal(0, net.Receive(1, delivered), "total loss delivers nothing");
            Equal(10, net.DroppedCount, "drop counter");

            CYGNetSimConfig duplicating = new CYGNetSimConfig
            {
                LatencyTicks = 1,
                DuplicatePerMille = 1000
            };
            CYGNetworkSimulator dup = new CYGNetworkSimulator(duplicating, 12UL);
            dup.Send(0, 1, payload, 0, payload.Length);
            Equal(1, dup.DuplicatedCount, "duplicate counter");

            delivered.Clear();
            for (int i = 0; i < 6; i++)
            {
                dup.Advance();
                dup.Receive(1, delivered);
            }
            Equal(2, delivered.Count, "duplicate delivers two copies");
            Require(delivered[0].Sequence != delivered[1].Sequence, "duplicates have distinct sequences");
        }

        private static void TestJitterBoundsAndDeterminism()
        {
            CYGNetSimConfig config = new CYGNetSimConfig
            {
                LatencyTicks = 2,
                JitterTicks = 2
            };
            CYGNetworkSimulator first = new CYGNetworkSimulator(config, 99UL);
            CYGNetworkSimulator second = new CYGNetworkSimulator(config, 99UL);
            byte[] payload = { 1 };
            for (int i = 0; i < 40; i++)
            {
                first.Send(0, 1, payload, 0, payload.Length);
                second.Send(0, 1, payload, 0, payload.Length);
            }

            List<CYGNetPacket> firstDelivery = new List<CYGNetPacket>();
            List<CYGNetPacket> secondDelivery = new List<CYGNetPacket>();
            for (int tick = 0; tick < 8; tick++)
            {
                first.Advance();
                second.Advance();
                first.Receive(1, firstDelivery);
                second.Receive(1, secondDelivery);
            }

            Equal(40, firstDelivery.Count, "all jittered packets arrive");
            Equal(40, secondDelivery.Count, "second simulator delivers the same count");
            for (int i = 0; i < 40; i++)
            {
                Require(firstDelivery[i].DeliveryTick >= 2 && firstDelivery[i].DeliveryTick <= 4,
                    "jitter stays inside the configured window");
                Equal(firstDelivery[i].DeliveryTick, secondDelivery[i].DeliveryTick,
                    "same seed produces the same delivery tick");
                Equal(firstDelivery[i].Sequence, secondDelivery[i].Sequence,
                    "same seed produces the same send order");
            }
        }

        private static void TestPayloadIsolationAndOrdering()
        {
            CYGNetSimConfig config = new CYGNetSimConfig { LatencyTicks = 1, JitterTicks = 1 };
            CYGNetworkSimulator net = new CYGNetworkSimulator(config, 5UL);
            byte[] payload = { 1, 2, 3 };
            net.Send(0, 1, payload, 0, payload.Length);
            payload[0] = 99;

            net.Send(0, 2, payload, 0, payload.Length);
            List<CYGNetPacket> toOne = new List<CYGNetPacket>();
            List<CYGNetPacket> toTwo = new List<CYGNetPacket>();
            net.Advance();
            net.Advance();
            Equal(1, net.Receive(1, toOne), "first receiver");
            Equal(1, net.Receive(2, toTwo), "second receiver");
            Equal((byte)1, toOne[0].Payload[0], "payload is copied on send");
            Equal((byte)99, toTwo[0].Payload[0], "second payload keeps its own bytes");

            CYGNetSimConfig reordered = new CYGNetSimConfig { LatencyTicks = 1, JitterTicks = 3 };
            CYGNetworkSimulator ordering = new CYGNetworkSimulator(reordered, 3UL);
            for (int i = 0; i < 20; i++)
            {
                byte[] one = { (byte)i };
                ordering.Send(0, 1, one, 0, 1);
            }
            List<CYGNetPacket> received = new List<CYGNetPacket>();
            for (int tick = 0; tick < 8; tick++)
            {
                ordering.Advance();
                ordering.Receive(1, received);
            }
            Equal(20, received.Count, "all ordered packets arrive");
            for (int i = 1; i < received.Count; i++)
            {
                bool orderedPair = received[i - 1].DeliveryTick < received[i].DeliveryTick
                    || (received[i - 1].DeliveryTick == received[i].DeliveryTick
                        && received[i - 1].Sequence < received[i].Sequence);
                Require(orderedPair, "receive order is (delivery tick, sequence)");
            }
        }

        private static void TestInputRedundancyRoundtrip()
        {
            CYGPlayerInput[] inputs =
            {
                new CYGPlayerInput(5, CYGPlayerInput.ButtonJump, 100, -200),
                new CYGPlayerInput(6, CYGPlayerInput.ButtonAction1, 250, 0),
                new CYGPlayerInput(7, 0, -1000, 1000)
            };

            byte[] buffer = new byte[256];
            int written = CYGInputRedundancy.WritePacket(buffer, 0, 2, inputs, 3);
            Equal(CYGInputRedundancy.HeaderSize + 3 * CYGPlayerInput.SizeInBytes, written, "packet size");

            CYGPlayerInput[] decoded = new CYGPlayerInput[CYGInputRedundancy.MaxInputs];
            int playerId;
            int count;
            Require(CYGInputRedundancy.TryReadPacket(buffer, 0, written, decoded, out playerId, out count),
                "packet decodes");
            Equal(2, playerId, "packet player");
            Equal(3, count, "packet input count");
            Equal(5, decoded[0].Tick, "first tick");
            Equal(7, decoded[2].Tick, "last tick");
            Require(decoded[0].Jump, "jump flag roundtrip");
            Equal((short)-1000, decoded[2].MoveX, "movement roundtrip");

            CYGInputRingBuffer ring = new CYGInputRingBuffer(64, 8);
            Equal(3, CYGInputRedundancy.ReceiveInto(buffer, 0, written, decoded, ring, out playerId),
                "inputs applied to ring");
            Require(ring.IsInputConfirmed(2, 6), "redundant input is confirmed");
            Equal(250, ring.GetInput(2, 6).MoveX, "ring payload");

            CYGPlayerInput[] scratch = new CYGPlayerInput[CYGInputRedundancy.MaxInputs];
            Equal(3, CYGInputRedundancy.ReceiveInto(buffer, 0, written, scratch, ring, out playerId),
                "duplicate packet is idempotent");
            Require(ring.IsInputConfirmed(2, 6), "duplicate keeps confirmation");
        }

        private static void TestInputRedundancyValidation()
        {
            CYGPlayerInput[] inputs = { new CYGPlayerInput(10, 0, 0, 0), new CYGPlayerInput(11, 0, 0, 0) };
            byte[] buffer = new byte[128];
            int written = CYGInputRedundancy.WritePacket(buffer, 0, 1, inputs, 2);

            CYGPlayerInput[] decoded = new CYGPlayerInput[CYGInputRedundancy.MaxInputs];
            int playerId;
            int count;
            Require(!CYGInputRedundancy.TryReadPacket(buffer, 0, written - 1, decoded, out playerId, out count),
                "truncated input packet is rejected");
            buffer[0] = 9;
            Require(!CYGInputRedundancy.TryReadPacket(buffer, 0, written, decoded, out playerId, out count),
                "unknown version is rejected");
            buffer[0] = CYGInputRedundancy.PacketVersion;

            CYGPlayerInput[] gapped = { new CYGPlayerInput(10, 0, 0, 0), new CYGPlayerInput(12, 0, 0, 0) };
            int gapSize = CYGInputRedundancy.WritePacket(buffer, 0, 1, gapped, 2);
            Require(!CYGInputRedundancy.TryReadPacket(buffer, 0, gapSize, decoded, out playerId, out count),
                "non-contiguous input ticks are rejected");

            Require(Throws<ArgumentOutOfRangeException>(() =>
            {
                CYGPlayerInput[] tooMany = new CYGPlayerInput[CYGInputRedundancy.MaxInputs + 1];
                CYGInputRedundancy.WritePacket(buffer, 0, 1, tooMany, tooMany.Length);
            }), "too many inputs are rejected");
        }

        private static void TestVerifiedFrameTracker()
        {
            CYGVerifiedFrameTracker tracker = new CYGVerifiedFrameTracker(8);
            int desyncEvents = 0;
            tracker.OnDesync += info => desyncEvents++;

            tracker.MarkPredicted(5, 0xABCD1234u);
            Require(tracker.MarkVerified(5, 0xABCD1234u), "matching checksum verifies");
            Require(tracker.IsVerified(5), "frame is verified");
            Equal(5, tracker.LastVerifiedTick, "last verified tick");
            Equal(0, tracker.PendingCount, "no pending frame");
            Require(!tracker.HasDesync, "no desync on match");

            tracker.MarkPredicted(6, 0x11111111u);
            Require(!tracker.MarkVerified(6, 0x22222222u), "mismatch fails verification");
            Require(tracker.HasDesync, "mismatch records a desync");
            Equal(6, tracker.FirstDesync.Tick, "desync tick");
            Equal(0x11111111u, tracker.FirstDesync.LocalChecksum, "desync local checksum");
            Equal(0x22222222u, tracker.FirstDesync.RemoteChecksum, "desync remote checksum");
            Equal(1, tracker.MismatchCount, "mismatch count");
            Equal(1, desyncEvents, "desync event");

            tracker.MarkPredicted(7, 0x77777777u);
            Equal(1, tracker.PendingCount, "unverified prediction is pending");
            Require(tracker.MarkVerified(8, 0x88888888u), "unpredicted frame verifies");
            Equal(8, tracker.LastVerifiedTick, "last verified tick advances without prediction");

            uint stored;
            Require(tracker.TryGetLocalChecksum(7, out stored), "local checksum lookup");
            Equal(0x77777777u, stored, "stored checksum");
            tracker.Clear();
            Require(!tracker.TryGetLocalChecksum(7, out stored), "clear drops frames");
            Require(!tracker.HasDesync, "clear resets desync state");
        }

        private static void TestRunnerNetcodeIntegration()
        {
            CYGRollbackRunner runner = new CYGRollbackRunner(64, 64, 16, 8);
            runner.Step();
            runner.Step();
            Equal(2, runner.CurrentSimTick, "two ticks simulated");

            uint predicted;
            Require(runner.VerifiedFrames.TryGetLocalChecksum(2, out predicted), "runner records predicted checksum");
            Require(runner.ConfirmFrame(2, predicted), "server checksum matches");
            Require(runner.VerifiedFrames.IsVerified(2), "runner frame verified");

            uint tickOne;
            Require(runner.VerifiedFrames.TryGetLocalChecksum(1, out tickOne), "tick 1 checksum available");
            Require(!runner.ConfirmFrame(1, tickOne ^ 1u), "mismatch is reported");
            Require(runner.VerifiedFrames.HasDesync, "runner exposes desync");

            CYGPlayerInput[] inputs =
            {
                new CYGPlayerInput(0, 0, 100, 0),
                new CYGPlayerInput(1, 0, 100, 0)
            };
            byte[] packet = new byte[128];
            int written = CYGInputRedundancy.WritePacket(packet, 0, 2, inputs, 2);
            Equal(2, runner.ApplyRedundantInputPacket(packet, 0, written), "redundant packet applied");
            Require(runner.TotalRollbacks > 0, "late redundant input triggers rollback");
            Require(runner.InputRingBuffer.IsInputConfirmed(2, 1), "remote input confirmed after replay");
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
    }
}
