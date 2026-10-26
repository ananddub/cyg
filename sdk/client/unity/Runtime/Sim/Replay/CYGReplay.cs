using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    public sealed class CYGReplayFrame
    {
        public readonly int Tick;
        public readonly CYGPlayerInput[] Inputs;
        public readonly int[] PlayerIds;
        public readonly int Count;
        public readonly uint Checksum;

        public CYGReplayFrame(
            int tick,
            CYGPlayerInput[] inputs,
            int[] playerIds,
            int count,
            uint checksum)
        {
            Tick = tick;
            Count = count;
            Inputs = new CYGPlayerInput[count];
            PlayerIds = new int[count];
            Array.Copy(inputs, Inputs, count);
            Array.Copy(playerIds, PlayerIds, count);
            Checksum = checksum;
        }
    }

    public sealed class CYGReplayKeyframe
    {
        public readonly int Tick;
        public readonly CYGWorldSnapshot Snapshot;

        public CYGReplayKeyframe(int tick, CYGWorldSnapshot snapshot)
        {
            Tick = tick;
            Snapshot = snapshot;
        }
    }

    /// <summary>
    /// Recorded input/checksum stream plus in-memory keyframes for scrubbing.
    /// The serialized form embeds a full world savegame at the first recorded
    /// tick, so a replay file loaded into an empty world can be played back
    /// without the original session.
    /// </summary>
    public sealed class CYGReplay
    {
        public const byte FileVersion = 1;
        public const int FileHeaderSize = 33;

        private readonly List<CYGReplayFrame> _frames = new List<CYGReplayFrame>();
        private readonly List<CYGReplayKeyframe> _keyframes = new List<CYGReplayKeyframe>();

        public CYGReplay(
            int tickRate,
            ulong seed,
            uint configFingerprint,
            int keyframeInterval)
        {
            if (tickRate < 1)
            {
                throw new ArgumentOutOfRangeException(nameof(tickRate));
            }
            if (keyframeInterval < 1)
            {
                throw new ArgumentOutOfRangeException(nameof(keyframeInterval));
            }

            TickRate = tickRate;
            Seed = seed;
            ConfigFingerprint = configFingerprint;
            KeyframeInterval = keyframeInterval;
        }

        public int TickRate { get; }

        public ulong Seed { get; }

        public uint ConfigFingerprint { get; }

        public int KeyframeInterval { get; }

        public int FrameCount
        {
            get { return _frames.Count; }
        }

        public int StartTick
        {
            get { return _frames.Count > 0 ? _frames[0].Tick : -1; }
        }

        public int EndTick
        {
            get { return _frames.Count > 0 ? _frames[_frames.Count - 1].Tick : -1; }
        }

        public int KeyframeCount
        {
            get { return _keyframes.Count; }
        }

        public IReadOnlyList<CYGReplayFrame> Frames
        {
            get { return _frames; }
        }

        /// <summary>Full-state snapshot at the first recorded tick, used by standalone replay files.</summary>
        public byte[] InitialSave { get; internal set; }

        public int InitialSaveTick { get; internal set; } = -1;

        public bool HasKeyframes
        {
            get { return _keyframes.Count > 0; }
        }

        internal void AddFrame(CYGReplayFrame frame)
        {
            if (_frames.Count > 0 && frame.Tick <= _frames[_frames.Count - 1].Tick)
            {
                throw new InvalidOperationException("Replay frames must be recorded in increasing tick order.");
            }
            _frames.Add(frame);
        }

        internal void AddKeyframe(int tick, CYGWorldSnapshot snapshot)
        {
            _keyframes.Add(new CYGReplayKeyframe(tick, snapshot));
        }

        internal void RemoveFramesBefore(int tick)
        {
            int remove = 0;
            while (remove < _frames.Count && _frames[remove].Tick < tick)
            {
                remove++;
            }
            if (remove > 0)
            {
                _frames.RemoveRange(0, remove);
            }
        }

        internal void PruneKeyframes(int floorTick)
        {
            if (_keyframes.Count == 0)
            {
                return;
            }

            int keepFrom = 0;
            for (int i = 0; i < _keyframes.Count; i++)
            {
                if (_keyframes[i].Tick <= floorTick)
                {
                    keepFrom = i;
                }
                else
                {
                    break;
                }
            }
            if (keepFrom > 0)
            {
                _keyframes.RemoveRange(0, keepFrom);
            }
        }

        public bool TryGetFrame(int tick, out CYGReplayFrame frame)
        {
            int low = 0;
            int high = _frames.Count - 1;
            while (low <= high)
            {
                int mid = low + ((high - low) >> 1);
                int value = _frames[mid].Tick;
                if (value == tick)
                {
                    frame = _frames[mid];
                    return true;
                }
                if (value < tick)
                {
                    low = mid + 1;
                }
                else
                {
                    high = mid - 1;
                }
            }

            frame = null;
            return false;
        }

        internal CYGReplayKeyframe FindKeyframeAtOrBefore(int tick)
        {
            CYGReplayKeyframe best = null;
            for (int i = 0; i < _keyframes.Count; i++)
            {
                if (_keyframes[i].Tick > tick)
                {
                    break;
                }
                best = _keyframes[i];
            }
            return best;
        }

        /// <summary>
        /// Builds a bounded clip for instant replay. The clip keeps the last
        /// keyframe at or before <paramref name="fromTick"/> (serialized as its
        /// initial savegame) plus every frame needed to reach the requested
        /// range, so both in-memory seeking and a serialized clip can play
        /// without the original session.
        /// </summary>
        public CYGReplay Slice(int fromTick, int toTick)
        {
            if (toTick < fromTick)
            {
                throw new ArgumentOutOfRangeException(nameof(toTick));
            }

            CYGReplay clip = new CYGReplay(TickRate, Seed, ConfigFingerprint, KeyframeInterval);
            if (_frames.Count == 0)
            {
                return clip;
            }

            int start = Math.Max(fromTick, StartTick);
            int end = Math.Min(toTick, EndTick);
            if (end < start)
            {
                return clip;
            }

            CYGReplayKeyframe anchor = FindKeyframeAtOrBefore(start);
            int frameFloor = start;
            if (anchor != null)
            {
                frameFloor = Math.Min(start, anchor.Tick + 1);
            }
            else if (InitialSave != null && InitialSaveTick <= start)
            {
                frameFloor = Math.Min(start, InitialSaveTick + 1);
            }

            for (int i = 0; i < _frames.Count; i++)
            {
                CYGReplayFrame frame = _frames[i];
                if (frame.Tick >= frameFloor && frame.Tick <= end)
                {
                    clip.AddFrame(frame);
                }
            }

            if (anchor != null)
            {
                AddSerializedAnchor(clip, anchor);
                for (int i = 0; i < _keyframes.Count; i++)
                {
                    CYGReplayKeyframe keyframe = _keyframes[i];
                    if (keyframe.Tick > anchor.Tick && keyframe.Tick <= end)
                    {
                        clip.AddKeyframe(keyframe.Tick, keyframe.Snapshot);
                    }
                }
            }
            else if (InitialSave != null && InitialSaveTick <= start)
            {
                clip.InitialSave = InitialSave;
                clip.InitialSaveTick = InitialSaveTick;
                for (int i = 0; i < _keyframes.Count; i++)
                {
                    CYGReplayKeyframe keyframe = _keyframes[i];
                    if (keyframe.Tick <= end)
                    {
                        clip.AddKeyframe(keyframe.Tick, keyframe.Snapshot);
                    }
                }
            }

            return clip;
        }

        private static void AddSerializedAnchor(CYGReplay clip, CYGReplayKeyframe anchor)
        {
            byte[] save = new byte[CYGWorldSerializer.ComputeSaveSize(anchor.Snapshot)];
            CYGWorldSerializer.WriteSnapshotTo(anchor.Snapshot, anchor.Tick, save, 0);
            clip.InitialSave = save;
            clip.InitialSaveTick = anchor.Tick;
            clip.AddKeyframe(anchor.Tick, anchor.Snapshot);
        }

        public int ComputeSize()
        {
            int size = FileHeaderSize + (InitialSave != null ? InitialSave.Length : 0);
            for (int i = 0; i < _frames.Count; i++)
            {
                size += 9 + _frames[i].Count * (4 + CYGPlayerInput.SizeInBytes);
            }
            return size;
        }

        public int WriteTo(byte[] buffer, int offset)
        {
            int size = ComputeSize();
            CYGWire.RequireWritable(buffer, offset, size);

            int written = 0;
            CYGWire.WriteByte(buffer, offset + written, FileVersion);
            written += 1;
            CYGWire.WriteInt32(buffer, offset + written, TickRate);
            written += 4;
            CYGWire.WriteUInt64(buffer, offset + written, Seed);
            written += 8;
            CYGWire.WriteUInt32(buffer, offset + written, ConfigFingerprint);
            written += 4;
            CYGWire.WriteInt32(buffer, offset + written, KeyframeInterval);
            written += 4;
            CYGWire.WriteInt32(buffer, offset + written, InitialSaveTick);
            written += 4;
            int saveLength = InitialSave != null ? InitialSave.Length : 0;
            CYGWire.WriteInt32(buffer, offset + written, saveLength);
            written += 4;
            CYGWire.WriteInt32(buffer, offset + written, _frames.Count);
            written += 4;

            if (saveLength > 0)
            {
                Array.Copy(InitialSave, 0, buffer, offset + written, saveLength);
                written += saveLength;
            }

            for (int i = 0; i < _frames.Count; i++)
            {
                CYGReplayFrame frame = _frames[i];
                CYGWire.WriteInt32(buffer, offset + written, frame.Tick);
                written += 4;
                CYGWire.WriteByte(buffer, offset + written, (byte)frame.Count);
                written += 1;
                CYGWire.WriteUInt32(buffer, offset + written, frame.Checksum);
                written += 4;
                for (int input = 0; input < frame.Count; input++)
                {
                    CYGWire.WriteInt32(buffer, offset + written, frame.PlayerIds[input]);
                    written += 4;
                    frame.Inputs[input].WriteTo(buffer, offset + written);
                    written += CYGPlayerInput.SizeInBytes;
                }
            }

            return written;
        }

        public static CYGReplay ReadFrom(byte[] buffer, int offset, int length)
        {
            CYGWire.RequireReadable(buffer, offset, length);
            if (length < FileHeaderSize)
            {
                throw new InvalidOperationException("Replay file is truncated.");
            }

            int read = 0;
            byte version = CYGWire.ReadByte(buffer, offset + read);
            read += 1;
            if (version != FileVersion)
            {
                throw new InvalidOperationException("Unsupported replay version " + version + ".");
            }

            int tickRate = CYGWire.ReadInt32(buffer, offset + read);
            read += 4;
            ulong seed = CYGWire.ReadUInt64(buffer, offset + read);
            read += 8;
            uint fingerprint = CYGWire.ReadUInt32(buffer, offset + read);
            read += 4;
            int keyframeInterval = CYGWire.ReadInt32(buffer, offset + read);
            read += 4;
            int initialTick = CYGWire.ReadInt32(buffer, offset + read);
            read += 4;
            int saveLength = CYGWire.ReadInt32(buffer, offset + read);
            read += 4;
            int frameCount = CYGWire.ReadInt32(buffer, offset + read);
            read += 4;

            if (tickRate < 1 || keyframeInterval < 1 || saveLength < 0 || frameCount < 0
                || saveLength > length - read)
            {
                throw new InvalidOperationException("Replay header is invalid.");
            }

            CYGReplay replay = new CYGReplay(tickRate, seed, fingerprint, keyframeInterval);
            if (saveLength > 0)
            {
                byte[] save = new byte[saveLength];
                Array.Copy(buffer, offset + read, save, 0, saveLength);
                replay.InitialSave = save;
                replay.InitialSaveTick = initialTick;
            }
            read += saveLength;

            CYGPlayerInput[] inputs = new CYGPlayerInput[255];
            int[] playerIds = new int[255];
            for (int i = 0; i < frameCount; i++)
            {
                if (length - read < 9)
                {
                    throw new InvalidOperationException("Replay frame header is truncated.");
                }

                int tick = CYGWire.ReadInt32(buffer, offset + read);
                read += 4;
                int count = CYGWire.ReadByte(buffer, offset + read);
                read += 1;
                uint checksum = CYGWire.ReadUInt32(buffer, offset + read);
                read += 4;
                if (tick < 0 || length - read < count * (4 + CYGPlayerInput.SizeInBytes))
                {
                    throw new InvalidOperationException("Replay frame is truncated.");
                }

                for (int input = 0; input < count; input++)
                {
                    playerIds[input] = CYGWire.ReadInt32(buffer, offset + read);
                    read += 4;
                    inputs[input] = CYGPlayerInput.ReadFrom(buffer, offset + read).Clamped();
                    read += CYGPlayerInput.SizeInBytes;
                }
                replay.AddFrame(new CYGReplayFrame(tick, inputs, playerIds, count, checksum));
            }

            return replay;
        }
    }
}
