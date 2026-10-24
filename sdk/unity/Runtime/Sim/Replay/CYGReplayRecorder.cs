using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Records per-tick inputs and post-tick checksums into a
    /// <see cref="CYGReplay"/>. Call <see cref="Record"/> after the tick has
    /// been simulated: the frame checksum describes the state at the end of
    /// that tick and the initial savegame anchors the first recorded frame.
    ///
    /// Snapshots every <c>keyframeInterval</c> ticks keep scrubbing cheap.
    /// When <c>maxFrames</c> is positive, old frames are pruned and the
    /// initial save is re-anchored to the newest keyframe at or before the
    /// pruning floor, so both in-memory and serialized playback stay valid.
    /// </summary>
    public sealed class CYGReplayRecorder
    {
        public const int DefaultKeyframeInterval = 120;

        private readonly CYGWorld _world;
        private readonly CYGReplay _replay;
        private readonly int _maxFrames;
        private int _firstTick = -1;
        private int _lastTick = -1;

        public CYGReplayRecorder(
            CYGWorld world,
            int tickRate,
            ulong seed,
            uint configFingerprint,
            int keyframeInterval = DefaultKeyframeInterval,
            int maxFrames = 0)
        {
            if (world == null)
            {
                throw new ArgumentNullException(nameof(world));
            }
            if (maxFrames < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(maxFrames));
            }

            _world = world;
            _replay = new CYGReplay(tickRate, seed, configFingerprint, keyframeInterval);
            _maxFrames = maxFrames;
        }

        public CYGReplay Replay
        {
            get { return _replay; }
        }

        public int FirstRecordedTick
        {
            get { return _firstTick; }
        }

        public int LastRecordedTick
        {
            get { return _lastTick; }
        }

        /// <summary>Records the current world state as the frame for <paramref name="tick"/>.</summary>
        public void Record(
            int tick,
            CYGPlayerInput[] inputs,
            int[] playerIds,
            int count)
        {
            if (_world == null)
            {
                throw new InvalidOperationException("Recorder has no world.");
            }
            Record(tick, inputs, playerIds, count, _world.ComputeChecksum());
        }

        public void Record(
            int tick,
            CYGPlayerInput[] inputs,
            int[] playerIds,
            int count,
            uint checksum)
        {
            if (tick < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(tick));
            }
            if (inputs == null)
            {
                throw new ArgumentNullException(nameof(inputs));
            }
            if (playerIds == null)
            {
                throw new ArgumentNullException(nameof(playerIds));
            }
            if (count < 0 || count > inputs.Length || count > playerIds.Length)
            {
                throw new ArgumentOutOfRangeException(nameof(count));
            }
            if (_lastTick >= 0 && tick <= _lastTick)
            {
                throw new InvalidOperationException(
                    "Replay frames must be recorded in increasing tick order (last " + _lastTick + ", got " + tick + ").");
            }

            if (_firstTick < 0)
            {
                _firstTick = tick;
                byte[] save = new byte[CYGWorldSerializer.ComputeSaveSize(_world)];
                CYGWorldSerializer.WriteTo(_world, tick, save, 0);
                _replay.InitialSave = save;
                _replay.InitialSaveTick = tick;
                _replay.AddKeyframe(tick, _world.CreateSnapshot());
            }

            _replay.AddFrame(new CYGReplayFrame(tick, inputs, playerIds, count, checksum));
            _lastTick = tick;

            if (tick != _firstTick
                && (tick - _firstTick) % _replay.KeyframeInterval == 0)
            {
                _replay.AddKeyframe(tick, _world.CreateSnapshot());
            }

            if (_maxFrames > 0 && _replay.FrameCount > _maxFrames)
            {
                Prune(tick);
            }
        }

        private void Prune(int tick)
        {
            int floor = tick - _maxFrames + 1;
            CYGReplayKeyframe anchor = _replay.FindKeyframeAtOrBefore(floor);
            if (anchor == null)
            {
                return;
            }

            _replay.RemoveFramesBefore(anchor.Tick + 1);
            _replay.PruneKeyframes(anchor.Tick);

            if (anchor.Tick > _replay.InitialSaveTick)
            {
                byte[] save = new byte[CYGWorldSerializer.ComputeSaveSize(anchor.Snapshot)];
                CYGWorldSerializer.WriteSnapshotTo(anchor.Snapshot, anchor.Tick, save, 0);
                _replay.InitialSave = save;
                _replay.InitialSaveTick = anchor.Tick;
            }
        }
    }
}
