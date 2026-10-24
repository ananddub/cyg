using System;

namespace CookYourGame.Sim.Tests
{
    internal static class ReplayTests
    {
        private const int ActorCount = 6;
        private const int RecordedTicks = 80;
        private const int KeyframeInterval = 16;

        private static int _assertions;

        public static int AssertionCount
        {
            get { return _assertions; }
        }

        public static void RunAll()
        {
            TestSavegameRoundtrip();
            TestSavegameValidation();
            TestReplayRecordAndSeek();
            TestReplayDetectsDesync();
            TestReplayFileRoundtrip();
            TestReplaySlice();
            TestBoundedRecorder();
            TestRecorderValidation();
        }

        private static void TestSavegameRoundtrip()
        {
            CYGEntityRef[] actors;
            CYGWorld source = CreateWorld(0xABCDEFUL, out actors);
            for (int tick = 0; tick < 25; tick++)
            {
                StepWorld(source, tick, actors);
            }

            int size = CYGWorldSerializer.ComputeSaveSize(source);
            byte[] save = new byte[size];
            int written = CYGWorldSerializer.WriteTo(source, 25, save, 0);
            Equal(size, written, "savegame wrote its computed size");

            CYGWorld restored = CreateEmptyWorld();
            int tickOut;
            uint checksum = CYGWorldSerializer.ReadFrom(restored, save, 0, save.Length, out tickOut);
            Equal(25, tickOut, "savegame tick");
            Equal(source.ComputeChecksum(), checksum, "restored checksum");
            Equal(source.ComputeChecksum(), restored.ComputeChecksum(), "restored world checksum");
            Equal(source.EntityCount, restored.EntityCount, "restored entity count");

            for (int tick = 25; tick < 45; tick++)
            {
                StepWorld(source, tick, actors);
                StepWorld(restored, tick, ActorCount);
                Equal(source.ComputeChecksum(), restored.ComputeChecksum(), "post-load simulation tick " + tick);
            }

            restored.Signals.Raise(3);
            Equal(restored.ComputeChecksum(), source.ComputeChecksum(), "signals must not affect the state checksum");
        }

        private static void TestSavegameValidation()
        {
            CYGEntityRef[] actors;
            CYGWorld source = CreateWorld(0x1234UL, out actors);
            for (int tick = 0; tick < 5; tick++)
            {
                StepWorld(source, tick, actors);
            }

            byte[] save = new byte[CYGWorldSerializer.ComputeSaveSize(source)];
            CYGWorldSerializer.WriteTo(source, 5, save, 0);

            CYGWorld wrongSchema = new CYGWorld();
            wrongSchema.Register<TestPosition>();
            ExpectThrows(
                () =>
                {
                    int ignored;
                    CYGWorldSerializer.ReadFrom(wrongSchema, save, 0, save.Length, out ignored);
                },
                "savegame store count must match the world");

            byte[] truncated = new byte[save.Length - 1];
            Array.Copy(save, truncated, truncated.Length);
            ExpectThrows(
                () =>
                {
                    CYGWorld restored = CreateEmptyWorld();
                    int ignored;
                    CYGWorldSerializer.ReadFrom(restored, truncated, 0, truncated.Length, out ignored);
                },
                "truncated savegame must be rejected");

            byte[] badVersion = (byte[])save.Clone();
            badVersion[0] = 99;
            ExpectThrows(
                () =>
                {
                    CYGWorld restored = CreateEmptyWorld();
                    int ignored;
                    CYGWorldSerializer.ReadFrom(restored, badVersion, 0, badVersion.Length, out ignored);
                },
                "unknown savegame version must be rejected");
        }

        private static void TestReplayRecordAndSeek()
        {
            CYGEntityRef[] actors;
            CYGWorld world = CreateWorld(0x5EEDUL, out actors);
            CYGReplayRecorder recorder = new CYGReplayRecorder(
                world, 60, 0x5EEDUL, 0xCAFEF00Du, KeyframeInterval);

            for (int tick = 0; tick < RecordedTicks; tick++)
            {
                StepWorld(world, tick, actors);
                recorder.Record(tick, BuildInputs(tick, actors.Length), BuildPlayerIds(actors), actors.Length);
            }

            CYGReplay replay = recorder.Replay;
            Equal(RecordedTicks, replay.FrameCount, "recorded frame count");
            Equal(0, replay.StartTick, "replay start tick");
            Equal(RecordedTicks - 1, replay.EndTick, "replay end tick");
            Require(replay.InitialSave != null, "recorder captured an initial savegame");
            Equal(0, replay.InitialSaveTick, "initial savegame tick");
            Equal(5, replay.KeyframeCount, "keyframes at 0,16,32,48,64");

            CYGWorld playback = CreateEmptyWorld();
            CYGReplayPlayer player = new CYGReplayPlayer(
                playback,
                replay,
                (w, tick, inputs, ids, count) => StepWorld(w, tick, ids, inputs, count));

            player.Reset();
            Equal(0, player.CurrentTick, "reset restores the first frame anchor");
            Equal(1, player.FrameIndex, "frame at the anchor tick is not simulated twice");

            Require(player.Seek(40), "seek to tick 40");
            Equal(40, player.CurrentTick, "seek lands on the target frame");

            Require(player.PlayToEnd(), "play to the end");
            Equal(RecordedTicks - 1, player.CurrentTick, "replay finished at the last tick");
            Require(player.IsAtEnd, "player reports end of replay");
            Require(player.LastMismatch == null, "no replay checksum mismatch");

            Require(player.Seek(5), "seek backwards restores an earlier keyframe");
            Equal(5, player.CurrentTick, "backwards seek tick");

            Require(player.Seek(RecordedTicks - 1), "seek to the end");
            Equal(RecordedTicks - 1, player.CurrentTick, "end seek tick");

            Require(!player.Step(), "stepping past the end returns false");
            Require(player.LastMismatch == null, "stepping past the end is not a desync");
        }

        private static void TestReplayDetectsDesync()
        {
            CYGEntityRef[] actors;
            CYGWorld world = CreateWorld(0xD00DUL, out actors);
            CYGReplayRecorder recorder = new CYGReplayRecorder(
                world, 60, 0xD00DUL, 1u, KeyframeInterval);
            for (int tick = 0; tick < 60; tick++)
            {
                StepWorld(world, tick, actors);
                recorder.Record(tick, BuildInputs(tick, actors.Length), BuildPlayerIds(actors), actors.Length);
            }

            CYGReplay replay = recorder.Replay;
            bool perturbed = false;
            CYGWorld playback = CreateEmptyWorld();
            CYGReplayPlayer player = new CYGReplayPlayer(
                playback,
                replay,
                (w, tick, inputs, ids, count) =>
                {
                    StepWorld(w, tick, ids, inputs, count);
                    if (tick == 42 && !perturbed)
                    {
                        perturbed = true;
                        CYGEntityRef entity;
                        TestPosition position;
                        if (w.TryGetEntityAt(ids[0], out entity) && w.TryGet<TestPosition>(entity, out position))
                        {
                            position.Value.y += FP.One;
                            w.Set(entity, position);
                        }
                    }
                });

            Require(!player.Seek(45), "perturbed replay must report a desync");
            Require(player.LastMismatch != null, "desync details are captured");
            Equal(42, player.LastMismatch.Tick, "first diverging tick");
            Require(
                player.LastMismatch.ExpectedChecksum != player.LastMismatch.ActualChecksum,
                "desync checksums differ");

            Require(player.Seek(45), "seeking again recovers from the mismatch");
            Require(player.LastMismatch == null, "recovery clears the mismatch");
        }

        private static void TestReplayFileRoundtrip()
        {
            CYGEntityRef[] actors;
            CYGWorld world = CreateWorld(0xF00DUL, out actors);
            CYGReplayRecorder recorder = new CYGReplayRecorder(
                world, 30, 0xF00DUL, 0x12345678u, KeyframeInterval);
            for (int tick = 0; tick < 50; tick++)
            {
                StepWorld(world, tick, actors);
                recorder.Record(tick, BuildInputs(tick, actors.Length), BuildPlayerIds(actors), actors.Length);
            }

            CYGReplay original = recorder.Replay;
            byte[] file = new byte[original.ComputeSize()];
            int written = original.WriteTo(file, 0);
            Equal(file.Length, written, "replay wrote its computed size");

            CYGReplay loaded = CYGReplay.ReadFrom(file, 0, file.Length);
            Equal(original.TickRate, loaded.TickRate, "replay tick rate");
            Equal((long)original.Seed, (long)loaded.Seed, "replay seed");
            Equal(original.ConfigFingerprint, loaded.ConfigFingerprint, "replay config fingerprint");
            Equal(original.KeyframeInterval, loaded.KeyframeInterval, "replay keyframe interval");
            Equal(original.FrameCount, loaded.FrameCount, "replay frame count");
            Equal(original.StartTick, loaded.StartTick, "replay file start tick");
            Equal(original.EndTick, loaded.EndTick, "replay file end tick");
            Require(loaded.InitialSave != null, "replay file embeds the initial savegame");
            Equal(original.InitialSaveTick, loaded.InitialSaveTick, "replay file initial save tick");

            CYGReplayFrame frame;
            Require(loaded.TryGetFrame(25, out frame), "loaded replay finds a frame");
            Equal(25, frame.Tick, "loaded frame tick");
            Equal(ActorCount, frame.Count, "loaded frame input count");

            CYGWorld playback = CreateEmptyWorld();
            CYGReplayPlayer player = new CYGReplayPlayer(
                playback,
                loaded,
                (w, tick, inputs, ids, count) => StepWorld(w, tick, ids, inputs, count));
            player.Reset();
            Require(player.PlayToEnd(), "loaded replay plays to the end without desync");

            byte[] badVersion = (byte[])file.Clone();
            badVersion[0] = 77;
            ExpectThrows(() => CYGReplay.ReadFrom(badVersion, 0, badVersion.Length), "bad replay version");

            byte[] truncated = new byte[file.Length - 4];
            Array.Copy(file, truncated, truncated.Length);
            ExpectThrows(() => CYGReplay.ReadFrom(truncated, 0, truncated.Length), "truncated replay file");
        }

        private static void TestReplaySlice()
        {
            CYGEntityRef[] actors;
            CYGWorld world = CreateWorld(0xBEEFUL, out actors);
            CYGReplayRecorder recorder = new CYGReplayRecorder(
                world, 60, 0xBEEFUL, 2u, KeyframeInterval);
            for (int tick = 0; tick < RecordedTicks; tick++)
            {
                StepWorld(world, tick, actors);
                recorder.Record(tick, BuildInputs(tick, actors.Length), BuildPlayerIds(actors), actors.Length);
            }

            CYGReplay clip = recorder.Replay.Slice(40, 60);
            Require(clip.FrameCount > 0, "slice retains frames");
            Equal(32, clip.InitialSaveTick, "slice anchors on the keyframe before the clip");
            Require(clip.InitialSave != null, "slice embeds a standalone initial savegame");
            Require(clip.StartTick <= 40, "slice retains the frames needed to reach the clip start");
            Equal(60, clip.EndTick, "slice end tick");
            Equal(2, clip.KeyframeCount, "slice keeps the anchor and the keyframe inside the clip");

            byte[] file = new byte[clip.ComputeSize()];
            clip.WriteTo(file, 0);
            CYGReplay loaded = CYGReplay.ReadFrom(file, 0, file.Length);

            CYGWorld playback = CreateEmptyWorld();
            CYGReplayPlayer player = new CYGReplayPlayer(
                playback,
                loaded,
                (w, tick, inputs, ids, count) => StepWorld(w, tick, ids, inputs, count));
            player.Reset();
            Require(player.PlayToEnd(), "serialized slice plays standalone without desync");
            Equal(60, player.CurrentTick, "slice playback reaches its last frame");

            Require(player.Seek(45), "seek inside the slice");
            Equal(45, player.CurrentTick, "slice seek tick");

            CYGReplay empty = recorder.Replay.Slice(500, 600);
            Equal(0, empty.FrameCount, "slice outside the recording is empty");
        }

        private static void TestBoundedRecorder()
        {
            CYGEntityRef[] actors;
            CYGWorld world = CreateWorld(0xC0DEUL, out actors);
            CYGReplayRecorder recorder = new CYGReplayRecorder(
                world, 60, 0xC0DEUL, 3u, keyframeInterval: 5, maxFrames: 10);
            for (int tick = 0; tick < 40; tick++)
            {
                StepWorld(world, tick, actors);
                recorder.Record(tick, BuildInputs(tick, actors.Length), BuildPlayerIds(actors), actors.Length);
            }

            CYGReplay replay = recorder.Replay;
            Require(replay.FrameCount <= 10, "bounded recorder keeps at most maxFrames frames");
            Equal(39, replay.EndTick, "bounded recorder keeps the newest tick");
            Equal(30, replay.InitialSaveTick, "bounded recorder re-anchors at the pruning keyframe");
            Require(replay.InitialSave != null, "bounded recorder keeps a standalone savegame");

            byte[] file = new byte[replay.ComputeSize()];
            replay.WriteTo(file, 0);
            CYGReplay loaded = CYGReplay.ReadFrom(file, 0, file.Length);

            CYGWorld playback = CreateEmptyWorld();
            CYGReplayPlayer player = new CYGReplayPlayer(
                playback,
                loaded,
                (w, tick, inputs, ids, count) => StepWorld(w, tick, ids, inputs, count));
            player.Reset();
            Require(player.PlayToEnd(), "pruned replay still plays standalone without desync");
            Require(player.Seek(35), "pruned replay can seek inside the retained window");
        }

        private static void TestRecorderValidation()
        {
            CYGEntityRef[] actors;
            CYGWorld world = CreateWorld(1UL, out actors);
            CYGReplayRecorder recorder = new CYGReplayRecorder(world, 60, 1UL, 0u);
            recorder.Record(5, BuildInputs(5, actors.Length), BuildPlayerIds(actors), actors.Length);
            ExpectThrows(
                () => recorder.Record(5, BuildInputs(5, actors.Length), BuildPlayerIds(actors), actors.Length),
                "duplicate tick must be rejected");
            ExpectThrows(
                () => recorder.Record(4, BuildInputs(4, actors.Length), BuildPlayerIds(actors), actors.Length),
                "decreasing tick must be rejected");
            ExpectThrows(
                () => recorder.Record(6, BuildInputs(6, actors.Length), BuildPlayerIds(actors), actors.Length + 1),
                "input count larger than the arrays must be rejected");
            ExpectThrows(() => new CYGReplayRecorder(null, 60, 0UL, 0u), "recorder requires a world");
            ExpectThrows(() => new CYGReplayRecorder(world, 0, 0UL, 0u), "tick rate must be positive");
            ExpectThrows(() => new CYGReplayRecorder(world, 60, 0UL, 0u, keyframeInterval: 0), "keyframe interval must be positive");
            ExpectThrows(() => new CYGReplayRecorder(world, 60, 0UL, 0u, maxFrames: -1), "max frames must not be negative");
        }

        private static CYGWorld CreateWorld(ulong seed, out CYGEntityRef[] actors)
        {
            CYGWorld world = CreateEmptyWorld();
            world.SeedRng(seed);
            actors = new CYGEntityRef[ActorCount];
            for (int i = 0; i < actors.Length; i++)
            {
                CYGEntityRef entity = world.CreateEntity();
                actors[i] = entity;
                world.Set(entity, new TestPosition
                {
                    Value = new FPVector3(FP.FromInt(i), FP.FromInt(2), FP.FromInt(-i))
                });
                world.Set(entity, new TestHealth { Value = FP.FromInt(100), Team = i & 1 });
                world.Set(entity, new TestLabel { Id = i });
            }
            return world;
        }

        private static CYGWorld CreateEmptyWorld()
        {
            CYGWorld world = new CYGWorld();
            world.Register<TestPosition>();
            world.Register<TestHealth>();
            world.Register<TestLabel>();
            return world;
        }

        private static int[] BuildPlayerIds(CYGEntityRef[] actors)
        {
            int[] ids = new int[actors.Length];
            for (int i = 0; i < actors.Length; i++)
            {
                ids[i] = actors[i].Index;
            }
            return ids;
        }

        private static CYGPlayerInput[] BuildInputs(int tick, int count)
        {
            CYGPlayerInput[] inputs = new CYGPlayerInput[count];
            for (int i = 0; i < inputs.Length; i++)
            {
                inputs[i] = new CYGPlayerInput(
                    tick,
                    (byte)((tick + i) & 3),
                    (short)(((tick + i * 3) % 11) - 5),
                    (short)(((tick * 5 + i) % 7) - 3));
            }
            return inputs;
        }

        private static void StepWorld(CYGWorld world, int tick, CYGEntityRef[] actors)
        {
            StepWorld(world, tick, BuildPlayerIds(actors), BuildInputs(tick, actors.Length), actors.Length);
        }

        /// <summary>Simulates actors stored in the first <paramref name="count"/> entity slots.</summary>
        private static void StepWorld(CYGWorld world, int tick, int count)
        {
            int[] ids = new int[count];
            for (int i = 0; i < count; i++)
            {
                ids[i] = i;
            }
            StepWorld(world, tick, ids, BuildInputs(tick, count), count);
        }

        private static void StepWorld(
            CYGWorld world,
            int tick,
            int[] playerIds,
            CYGPlayerInput[] inputs,
            int count)
        {
            int noise = world.Rng.NextInt(-2, 3);
            for (int i = 0; i < count; i++)
            {
                CYGEntityRef entity;
                if (!world.TryGetEntityAt(playerIds[i], out entity))
                {
                    continue;
                }

                TestPosition position;
                if (!world.TryGet<TestPosition>(entity, out position))
                {
                    continue;
                }

                CYGPlayerInput input = inputs[i];
                position.Value = new FPVector3(
                    position.Value.x + FP.FromInt(input.MoveX),
                    position.Value.y + FP.FromInt(input.MoveY) + FP.FromRaw(noise),
                    position.Value.z + FP.FromInt((input.Buttons & 1) * 2));
                world.Set(entity, position);

                if ((tick & 7) == 0)
                {
                    TestHealth health;
                    if (world.TryGet<TestHealth>(entity, out health))
                    {
                        health.Value += FP.FromInt(1);
                        world.Set(entity, health);
                    }
                }
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

        private static void Equal(int expected, int actual, string message)
        {
            _assertions++;
            if (expected != actual)
            {
                throw new InvalidOperationException(message + ": expected " + expected + ", got " + actual);
            }
        }

        private static void Equal(uint expected, uint actual, string message)
        {
            _assertions++;
            if (expected != actual)
            {
                throw new InvalidOperationException(
                    message + ": expected 0x" + expected.ToString("X8") + ", got 0x" + actual.ToString("X8"));
            }
        }

        private static void Equal(long expected, long actual, string message)
        {
            _assertions++;
            if (expected != actual)
            {
                throw new InvalidOperationException(message + ": expected " + expected + ", got " + actual);
            }
        }

        private static void ExpectThrows(Action action, string message)
        {
            _assertions++;
            try
            {
                action();
            }
            catch (Exception)
            {
                return;
            }
            throw new InvalidOperationException(message + ": expected an exception");
        }
    }
}
