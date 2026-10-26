using System;
using System.Runtime.InteropServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Automated test suite for the Rollback and State Simulation engine.
    /// Validates zero GC allocations, snapshot ring buffers, CRC32 checksums, and rollback resimulation.
    /// </summary>
    public static class RollbackSimTest
    {
        public static void RunAllTests()
        {
            Console.WriteLine("\n=================================================");
            Console.WriteLine(" CookYourGame Rollback & State Simulation Tests");
            Console.WriteLine("=================================================");

            TestPlayerInputContract();
            TestInputRingBuffer();
            TestInputValidationAndActorMapping();
            TestStateRingBuffer();
            TestChecksum();
            TestRollbackRunnerReplay();
            TestZeroGCExecution();

            Console.WriteLine(">>> SUCCESS: All Rollback & State Engine Tests Passed! <<<");
        }

        private static void Assert(bool condition, string message)
        {
            if (!condition)
            {
                Console.ForegroundColor = ConsoleColor.Red;
                Console.WriteLine($"[FAIL] {message}");
                Console.ResetColor();
                throw new Exception($"Assertion failed: {message}");
            }
        }

        private static void Pass(string message)
        {
            Console.ForegroundColor = ConsoleColor.Green;
            Console.WriteLine($"[PASS] {message}");
            Console.ResetColor();
        }

        private static void TestPlayerInputContract()
        {
            int size = Marshal.SizeOf<CYGPlayerInput>();
            Assert(size == 9, $"CYGPlayerInput size must be exactly 9 bytes, got {size}");
            Pass($"CYGPlayerInput struct size = {size} bytes (Pack = 1 sequential)");

            CYGPlayerInput input = new CYGPlayerInput(105, 0, -500, 750);
            input.Jump = true;
            input.Action1 = true;

            Assert(input.Jump, "Jump button should be set");
            Assert(input.Action1, "Action1 button should be set");
            Assert(!input.Action2, "Action2 button should NOT be set");
            Assert(Math.Abs(input.NormalizedMoveX - (-0.5f)) < 0.001f, "NormalizedMoveX should be -0.5");
            Assert(Math.Abs(input.NormalizedMoveY - 0.75f) < 0.001f, "NormalizedMoveY should be +0.75");

            byte[] buffer = new byte[16];
            input.WriteTo(buffer, 3);
            CYGPlayerInput readBack = CYGPlayerInput.ReadFrom(buffer, 3);

            Assert(input == readBack, "Serialized/Deserialized CYGPlayerInput must match exactly");
            Pass("CYGPlayerInput serialization roundtrip verified");
        }

        private static void TestInputRingBuffer()
        {
            CYGInputRingBuffer ring = new CYGInputRingBuffer(capacity: 128, maxPlayers: 4);

            for (int t = 0; t <= 5; t++)
            {
                ring.SetInput(1, t, new CYGPlayerInput(t, (byte)t, (short)(t * 100), (short)(t * -100)), isPredicted: false);
            }

            for (int t = 0; t <= 5; t++)
            {
                CYGPlayerInput retrieved = ring.GetInput(1, t);
                Assert(retrieved.Tick == t, $"Tick mismatch at {t}");
                Assert(retrieved.Buttons == (byte)t, $"Buttons mismatch at {t}");
                Assert(ring.IsInputConfirmed(1, t), $"Input at tick {t} should be confirmed");
                Assert(!ring.IsInputPredicted(1, t), $"Input at tick {t} should not be predicted");
            }
            Pass("Confirmed input retrieval verified");

            // Predict missing tick 6 (repeat last known input from tick 5)
            CYGPlayerInput predicted = ring.GetInput(1, 6);
            Assert(predicted.Tick == 6, "Predicted input should have tick 6");
            Assert(predicted.Buttons == 5, "Predicted input should repeat buttons from tick 5");
            Assert(predicted.MoveX == 500, "Predicted input should repeat MoveX from tick 5");
            Assert(ring.IsInputPredicted(1, 6), "Input at tick 6 should be marked as predicted");
            Pass("Input prediction (repeat last input) verified");

            // Modulo wrapping beyond capacity
            for (int t = 6; t < 200; t++)
            {
                ring.SetInput(1, t, new CYGPlayerInput(t, 1, 200, 300), isPredicted: false);
            }
            CYGPlayerInput wrapped = ring.GetInput(1, 199);
            Assert(wrapped.Tick == 199, "Wrapped input should retrieve correct tick 199");
            Pass("InputRingBuffer modulo wrapping verified (200 ticks across capacity 128)");
        }

        private static void TestInputValidationAndActorMapping()
        {
            CYGPlayerInput clamped = new CYGPlayerInput(
                0,
                0xFF,
                short.MaxValue,
                short.MinValue);
            Assert(clamped.Buttons == CYGPlayerInput.ButtonMask, "Unknown button bits must be masked");
            Assert(clamped.MoveX == CYGPlayerInput.MaxMove, "MoveX must clamp to +1000");
            Assert(clamped.MoveY == -CYGPlayerInput.MaxMove, "MoveY must clamp to -1000");

            CYGPlayerInput mutated = new CYGPlayerInput(0, 0, 0, 0);
            mutated.MoveX = short.MaxValue;
            mutated.MoveY = short.MinValue;
            mutated.Buttons = 0xFF;
            CYGPlayerInput safe = mutated.Clamped();
            Assert(safe.MoveX == CYGPlayerInput.MaxMove, "Clamped() must clamp MoveX");
            Assert(safe.MoveY == -CYGPlayerInput.MaxMove, "Clamped() must clamp MoveY");
            Assert(safe.Buttons == CYGPlayerInput.ButtonMask, "Clamped() must mask buttons");
            Pass("Untrusted input ranges are clamped at the trust boundary");

            CYGInputRingBuffer ring = new CYGInputRingBuffer(capacity: 128, maxPlayers: 4);
            ring.SetInput(1, 0, new CYGPlayerInput(0, 0, 1000, 0));
            ring.SetInput(1, 2, new CYGPlayerInput(2, 0, 1000, 0));
            ring.SetInput(1, 3, new CYGPlayerInput(3, 0, 1000, 0));
            Assert(
                ring.GetLowestConfirmedTick(-1) == 0,
                "A gap at tick 1 must keep the contiguous confirmed tick at 0");
            ring.SetInput(1, 1, new CYGPlayerInput(1, 0, 1000, 0));
            Assert(
                ring.GetLowestConfirmedTick(-1) == 3,
                "Filling the gap must advance the contiguous confirmed tick to 3");

            ring.SetInput(2, 100, new CYGPlayerInput(100, 0, 0, 0));
            ring.SetInput(2, 50, new CYGPlayerInput(50, 0, 0, 0));
            Assert(
                ring.GetLowestConfirmedTick(-1) == 3,
                "A late pre-first packet must not drag confirmed ticks backwards");
            Pass("Confirmed ticks advance contiguously across packet loss");

            CYGRollbackRunner runner = new CYGRollbackRunner();
            // Actor ids are hashes/seat ids, not dense 1..N indexes.
            runner.LocalPlayerId = 7;
            runner.SetEntityState(new CYGEntityState(7, FPVector3.Zero));
            runner.SetEntityState(new CYGEntityState(42, new FPVector3(10, 0, 0)));

            Assert(
                !runner.OnRemoteInputReceived(42, new CYGPlayerInput(int.MaxValue, 0, 0, 0)),
                "Future tick must be rejected");

            // The second client's input arrives first. The simulation must
            // still map by actor id, not by array position.
            CYGPlayerInput[] inputs =
            {
                new CYGPlayerInput(0, 0, -1000, 0), // actor 42
                new CYGPlayerInput(0, 0, 1000, 0),  // actor 7
            };
            int[] actorIds = { 42, 7 };
            runner.StepSimulation(0, inputs, actorIds);

            runner.TryGetEntityState(7, out CYGEntityState local);
            runner.TryGetEntityState(42, out CYGEntityState remote);
            Assert(local.Position.x > FP.Zero, "Actor 7 must move right regardless of arrival order");
            Assert(remote.Position.x < FP.FromInt(10), "Actor 42 must move left regardless of arrival order");
            Pass("Sparse actor-to-entity mapping is arrival-order independent");
        }

        private static void TestStateRingBuffer()
        {
            CYGStateRingBuffer stateRing = new CYGStateRingBuffer(capacity: 128, maxEntities: 32);

            CYGEntityState[] entities = new CYGEntityState[2];
            entities[0] = new CYGEntityState(1, new FPVector3(1, 2, 3), new FPVector3(4, 5, 6), FPQuaternion.Identity, 100);
            entities[1] = new CYGEntityState(2, new FPVector3(7, 8, 9), new FPVector3(0, 0, 0), FPQuaternion.Identity, 80);

            for (int t = 0; t < 100; t++)
            {
                entities[0].Position.x = FP.FromInt(t);
                uint crc = (uint)(t + 1000);
                stateRing.SaveSnapshot(t, entities, 2, crc);
            }

            Assert(stateRing.EarliestTick == 0, $"Earliest tick should be 0, got {stateRing.EarliestTick}");
            Assert(stateRing.LatestTick == 99, $"Latest tick should be 99, got {stateRing.LatestTick}");

            CYGEntityState[] restored = new CYGEntityState[32];
            bool success = stateRing.TryRestoreSnapshot(42, restored, out int count, out uint checksum);
            Assert(success, "Failed to restore snapshot at tick 42");
            Assert(count == 2, $"Expected 2 restored entities, got {count}");
            Assert(checksum == 1042u, $"Expected checksum 1042, got {checksum}");
            Assert(restored[0].Position.x == FP.FromInt(42), $"Restored position mismatch: {restored[0].Position.x}");
            Assert(restored[1].Health == 80, $"Restored health mismatch: {restored[1].Health}");
            Pass("StateRingBuffer save and restoration verified with exact data preservation");
        }

        private static void TestChecksum()
        {
            CYGEntityState[] entities = new CYGEntityState[2];
            entities[0] = new CYGEntityState(1, new FPVector3(10, 0, 5), new FPVector3(1, 0, 0), FPQuaternion.Identity, 100);
            entities[1] = new CYGEntityState(2, new FPVector3(-3, 0, 12), FPVector3.Zero, FPQuaternion.Identity, 95);

            uint crcA = CYGChecksum.Compute(50, entities, 2);
            uint crcB = CYGChecksum.Compute(50, entities, 2);
            Assert(crcA == crcB, "Checksum must be completely deterministic for identical states");
            Pass($"Deterministic CRC32 Checksum computed: {CYGChecksum.ToHexString(crcA)}");

            entities[1].Health = 94;
            uint crcC = CYGChecksum.Compute(50, entities, 2);
            Assert(crcA != crcC, "Checksum MUST differ when any entity field changes");
            Pass($"Desync instantly detected on single field variation (0x{crcA:X8} vs 0x{crcC:X8})");

            uint fnvA = CYGChecksum.ComputeFNV1a(50, entities, 2);
            Assert(fnvA != 0, "FNV-1a must return valid non-zero hash");
            Pass($"FNV-1a Hash computed: {CYGChecksum.ToHexString(fnvA)}");
        }

        private static void TestRollbackRunnerReplay()
        {
            CYGRollbackRunner runner = new CYGRollbackRunner();
            runner.LocalPlayerId = 1;
            int player1Id = runner.AddEntity(FPVector3.Zero, 100);
            int player2Id = runner.AddEntity(new FPVector3(10, 0, 0), 100);

            for (int t = 0; t < 5; t++)
            {
                runner.SetLocalInput(t, new CYGPlayerInput(t, 0, 1000, 0));
                runner.InputRingBuffer.SetInput(player2Id, t, new CYGPlayerInput(t, 0, 0, 0));
            }

            for (int t = 0; t < 10; t++)
            {
                if (t >= 5)
                {
                    runner.SetLocalInput(t, new CYGPlayerInput(t, 0, 1000, 0));
                }
                runner.Step();
            }

            Assert(runner.CurrentSimTick == 10, $"CurrentSimTick should be 10, got {runner.CurrentSimTick}");

            // Late remote packet arrives for Player 2 at past Tick 5: Player 2 jumped at tick 5
            CYGPlayerInput lateInput = new CYGPlayerInput(5, CYGPlayerInput.ButtonJump, 0, 0);
            bool rollbackOccurred = runner.OnRemoteInputReceived(player2Id, lateInput);

            Assert(rollbackOccurred, "OnRemoteInputReceived for past tick 5 MUST trigger rollback");
            Assert(runner.CurrentSimTick == 10, $"Simulation must resimulate back to CurrentSimTick (10), got {runner.CurrentSimTick}");
            Assert(runner.TotalRollbacks == 1, $"Total rollbacks should be 1, got {runner.TotalRollbacks}");
            Assert(runner.TotalResimulatedTicks == 5, $"Resimulated ticks should be 5 (ticks 5..9), got {runner.TotalResimulatedTicks}");

            runner.TryGetEntityState(player2Id, out CYGEntityState p2AfterRollback);
            Assert(p2AfterRollback.Position.y > FP.Zero, "Player 2 should now be in the air due to resimulated jump!");
            Pass($"Rollback resimulation successful! P2 Position Y after rollback: {p2AfterRollback.Position.y} (Jump replayed)");
        }

        private static void TestZeroGCExecution()
        {
            CYGRollbackRunner runner = new CYGRollbackRunner();
            runner.AddEntity(FPVector3.Zero, 100);
            runner.AddEntity(new FPVector3(5, 0, 5), 100);

            for (int i = 0; i < 50; i++)
            {
                runner.SetLocalInput(runner.CurrentSimTick, new CYGPlayerInput(runner.CurrentSimTick, 0, 500, 500));
                runner.Step();
            }

            GC.Collect();
            GC.WaitForPendingFinalizers();
            GC.Collect();

            long startAllocated = GC.GetAllocatedBytesForCurrentThread();
            int gc0Start = GC.CollectionCount(0);

            const int TestTicks = 500;
            for (int i = 0; i < TestTicks; i++)
            {
                runner.SetLocalInput(runner.CurrentSimTick, new CYGPlayerInput(runner.CurrentSimTick, 0, 1000, 0));
                runner.Step();

                if (i % 50 == 25)
                {
                    int pastTick = runner.CurrentSimTick - 5;
                    runner.OnRemoteInputReceived(2, new CYGPlayerInput(pastTick, CYGPlayerInput.ButtonJump, 0, 0));
                }
            }

            long bytesAllocated = GC.GetAllocatedBytesForCurrentThread() - startAllocated;
            int gc0End = GC.CollectionCount(0);

            Console.WriteLine($"Ran {TestTicks} ticks (+ rollbacks): Allocated = {bytesAllocated} bytes, GC Gen0 Collections = {gc0End - gc0Start}");
            Assert(gc0End == gc0Start, "Zero GC collections allowed during simulation ticks!");
            Pass("Zero-GC constraint verified! 0 GC collections during simulation ticks and rollbacks!");
        }
    }
}
