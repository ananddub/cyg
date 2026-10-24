using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>Details of the first frame whose replayed checksum diverged.</summary>
    public sealed class CYGReplayMismatch
    {
        public readonly int Tick;
        public readonly uint ExpectedChecksum;
        public readonly uint ActualChecksum;

        public CYGReplayMismatch(int tick, uint expectedChecksum, uint actualChecksum)
        {
            Tick = tick;
            ExpectedChecksum = expectedChecksum;
            ActualChecksum = actualChecksum;
        }
    }

    /// <summary>
    /// Replays a <see cref="CYGReplay"/> into a world. The simulation delegate
    /// receives the recorded inputs exactly like the original session; after
    /// each frame the world checksum is compared with the recording.
    ///
    /// Seeking restores the newest keyframe at or before the target (or the
    /// embedded initial savegame), then simulates forward. The world must
    /// already have the same component stores registered.
    /// </summary>
    public sealed class CYGReplayPlayer
    {
        public delegate void SimulationStep(
            CYGWorld world,
            int tick,
            CYGPlayerInput[] inputs,
            int[] playerIds,
            int count);

        private readonly CYGWorld _world;
        private readonly CYGReplay _replay;
        private readonly SimulationStep _simulate;
        private int _frameIndex;
        private int _currentTick = -1;
        private CYGReplayMismatch _lastMismatch;

        public CYGReplayPlayer(CYGWorld world, CYGReplay replay, SimulationStep simulate)
        {
            if (world == null)
            {
                throw new ArgumentNullException(nameof(world));
            }
            if (replay == null)
            {
                throw new ArgumentNullException(nameof(replay));
            }
            if (simulate == null)
            {
                throw new ArgumentNullException(nameof(simulate));
            }

            _world = world;
            _replay = replay;
            _simulate = simulate;
        }

        public CYGReplay Replay
        {
            get { return _replay; }
        }

        /// <summary>Tick of the last simulated frame, or the anchor tick after a reset.</summary>
        public int CurrentTick
        {
            get { return _currentTick; }
        }

        /// <summary>Index of the next frame to simulate.</summary>
        public int FrameIndex
        {
            get { return _frameIndex; }
        }

        public bool IsAtEnd
        {
            get { return _frameIndex >= _replay.FrameCount; }
        }

        /// <summary>First checksum mismatch since the last reset, or null.</summary>
        public CYGReplayMismatch LastMismatch
        {
            get { return _lastMismatch; }
        }

        /// <summary>
        /// Restores the earliest available anchor and verifies its checksum
        /// when the frame at that tick was recorded. Throws when the replay
        /// has no keyframe and no usable initial savegame.
        /// </summary>
        public void Reset()
        {
            _lastMismatch = null;
            _frameIndex = 0;
            _currentTick = -1;

            if (_replay.FrameCount == 0)
            {
                throw new InvalidOperationException("Replay has no frames.");
            }

            int startTick = _replay.StartTick;
            CYGReplayKeyframe anchor = _replay.FindKeyframeAtOrBefore(startTick);
            if (anchor != null)
            {
                RestoreKeyframe(anchor);
                return;
            }
            if (_replay.InitialSave != null && _replay.InitialSaveTick <= startTick)
            {
                RestoreInitialSave();
                return;
            }

            throw new InvalidOperationException(
                "Replay has no keyframe or initial savegame at or before tick " + startTick + ".");
        }

        /// <summary>
        /// Restores the nearest anchor at or before <paramref name="tick"/> and
        /// simulates every recorded frame up to it. Returns false if any
        /// checksum mismatched; <see cref="LastMismatch"/> identifies the frame.
        /// </summary>
        public bool Seek(int tick)
        {
            if (tick < _replay.StartTick || tick > _replay.EndTick)
            {
                throw new ArgumentOutOfRangeException(nameof(tick));
            }

            _lastMismatch = null;

            CYGReplayKeyframe anchor = _replay.FindKeyframeAtOrBefore(tick);
            if (anchor != null)
            {
                RestoreKeyframe(anchor);
            }
            else if (_replay.InitialSave != null && _replay.InitialSaveTick <= tick)
            {
                RestoreInitialSave();
            }
            else
            {
                Reset();
            }

            if (!VerifyCurrentTick())
            {
                return false;
            }

            while (_frameIndex < _replay.FrameCount)
            {
                CYGReplayFrame frame = _replay.Frames[_frameIndex];
                if (frame.Tick > tick)
                {
                    break;
                }
                if (!StepFrame(frame))
                {
                    return false;
                }
            }

            return true;
        }

        /// <summary>
        /// Simulates the next recorded frame. Returns false at the end of the
        /// replay or when the checksum diverged.
        /// </summary>
        public bool Step()
        {
            if (_currentTick < 0)
            {
                Reset();
                if (!VerifyCurrentTick())
                {
                    return false;
                }
            }

            if (_frameIndex >= _replay.FrameCount)
            {
                return false;
            }

            return StepFrame(_replay.Frames[_frameIndex]);
        }

        /// <summary>Simulates every remaining frame. Returns false on mismatch.</summary>
        public bool PlayToEnd()
        {
            while (!IsAtEnd)
            {
                if (!Step())
                {
                    return false;
                }
            }
            return _lastMismatch == null;
        }

        private bool StepFrame(CYGReplayFrame frame)
        {
            _simulate(
                _world,
                frame.Tick,
                frame.Inputs,
                frame.PlayerIds,
                frame.Count);
            _currentTick = frame.Tick;
            _frameIndex++;

            uint actual = _world.ComputeChecksum();
            if (actual != frame.Checksum)
            {
                _lastMismatch = new CYGReplayMismatch(frame.Tick, frame.Checksum, actual);
                return false;
            }
            return true;
        }

        private void RestoreKeyframe(CYGReplayKeyframe keyframe)
        {
            _world.Restore(keyframe.Snapshot);
            ClearSessionData();
            _currentTick = keyframe.Tick;
            _frameIndex = FirstFrameAfter(keyframe.Tick);
        }

        private void RestoreInitialSave()
        {
            int tick;
            CYGWorldSerializer.ReadFrom(
                _world,
                _replay.InitialSave,
                0,
                _replay.InitialSave.Length,
                out tick);
            _currentTick = tick;
            _frameIndex = FirstFrameAfter(tick);
        }

        private bool VerifyCurrentTick()
        {
            CYGReplayFrame frame;
            if (!_replay.TryGetFrame(_currentTick, out frame))
            {
                return true;
            }

            uint actual = _world.ComputeChecksum();
            if (actual != frame.Checksum)
            {
                _lastMismatch = new CYGReplayMismatch(_currentTick, frame.Checksum, actual);
                return false;
            }
            return true;
        }

        private int FirstFrameAfter(int tick)
        {
            IReadOnlyList<CYGReplayFrame> frames = _replay.Frames;
            int low = 0;
            int high = frames.Count;
            while (low < high)
            {
                int mid = low + ((high - low) >> 1);
                if (frames[mid].Tick <= tick)
                {
                    low = mid + 1;
                }
                else
                {
                    high = mid;
                }
            }
            return low;
        }

        private void ClearSessionData()
        {
            _world.Events.Clear();
            _world.Signals.Clear();
            _world.Commands.Clear();
        }
    }
}
