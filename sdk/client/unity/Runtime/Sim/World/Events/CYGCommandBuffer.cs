using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Per-player command ring. Commands are external input: rollback never
    /// rewinds them, and a repeated command keeps executing on later ticks.
    /// Prediction is expressed with <see cref="CYGCommandFlags.Predictable"/>
    /// and confirmation with <see cref="MarkVerified"/>.
    /// </summary>
    public sealed class CYGCommandBuffer
    {
        public const int PacketVersion = 1;
        public const int MaxSupportedPlayers = 256;
        public const int PacketHeaderSize = 13;

        private struct Slot
        {
            public CYGCommand Command;
            public int Tick;
            public bool Verified;
        }

        private readonly Slot[][] _slots;
        private readonly int[] _verifiedTicks;
        private readonly Dictionary<int, Func<CYGCommand>> _factories =
            new Dictionary<int, Func<CYGCommand>>();
        private readonly int _maxPlayers;
        private readonly int _capacity;
        private readonly int _repeatLookback;

        public CYGCommandBuffer(
            int maxPlayers = CYGDeterminismConfig.DefaultMaxPlayers,
            int capacity = CYGDeterminismConfig.DefaultRollbackCapacity,
            int repeatLookback = 32)
        {
            if (maxPlayers < 1 || maxPlayers > MaxSupportedPlayers)
            {
                throw new ArgumentOutOfRangeException(nameof(maxPlayers));
            }
            if (capacity < 1)
            {
                throw new ArgumentOutOfRangeException(nameof(capacity));
            }
            if (repeatLookback < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(repeatLookback));
            }

            _maxPlayers = maxPlayers;
            _capacity = capacity;
            _repeatLookback = repeatLookback;
            _slots = new Slot[maxPlayers][];
            for (int i = 0; i < maxPlayers; i++)
            {
                _slots[i] = new Slot[capacity];
            }
            _verifiedTicks = new int[maxPlayers];
            for (int i = 0; i < maxPlayers; i++)
            {
                _verifiedTicks[i] = -1;
            }
        }

        public int MaxPlayers
        {
            get { return _maxPlayers; }
        }

        public int Capacity
        {
            get { return _capacity; }
        }

        public void RegisterCommand(int typeId, Func<CYGCommand> factory)
        {
            if (factory == null)
            {
                throw new ArgumentNullException(nameof(factory));
            }
            FactoryMustProduce(typeId, factory);
            _factories[typeId] = factory;
        }

        public void AddCommand(int playerId, int tick, CYGCommand command)
        {
            if (command == null)
            {
                throw new ArgumentNullException(nameof(command));
            }
            if (tick < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(tick));
            }
            if (command.PayloadSize < 0 || command.PayloadSize > CYGCommand.MaxPayloadSize)
            {
                throw new ArgumentOutOfRangeException(nameof(command), "Command payload exceeds the wire limit.");
            }
            RequirePlayer(playerId);

            command.PlayerId = playerId;
            command.Tick = tick;

            int index = tick % _capacity;
            ref Slot slot = ref _slots[playerId][index];
            slot.Command = command;
            slot.Tick = tick;
            slot.Verified = false;
        }

        public bool TryGetCommand(int playerId, int tick, out CYGCommand command)
        {
            RequirePlayer(playerId);
            if (tick < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(tick));
            }

            ref Slot slot = ref _slots[playerId][tick % _capacity];
            if (slot.Command != null && slot.Tick == tick)
            {
                command = slot.Command;
                return true;
            }

            command = null;
            return false;
        }

        /// <summary>
        /// Resolves the command to execute: the exact tick when present, or the
        /// most recent repeatable command within the lookback window. Returns
        /// false for disabled commands and for ignored commands while
        /// re-simulating.
        /// </summary>
        public bool TryGetExecutableCommand(
            int playerId,
            int tick,
            bool isResimulating,
            out CYGCommand command)
        {
            if (TryGetCommand(playerId, tick, out command))
            {
                return IsExecutable(command, isResimulating);
            }

            int first = tick - _repeatLookback;
            if (first < 0)
            {
                first = 0;
            }
            for (int candidate = tick - 1; candidate >= first; candidate--)
            {
                CYGCommand previous;
                if (!TryGetCommand(playerId, candidate, out previous))
                {
                    continue;
                }

                if (!previous.IsRepeat)
                {
                    break;
                }

                command = previous;
                return IsExecutable(previous, isResimulating);
            }

            command = null;
            return false;
        }

        public bool IsExecutable(CYGCommand command, bool isResimulating)
        {
            if (command == null || command.IsDisabled)
            {
                return false;
            }
            return !(isResimulating && command.IsIgnoredOnResimulate);
        }

        /// <summary>Marks every command at or before <paramref name="tick"/> as confirmed.</summary>
        public void MarkVerified(int playerId, int tick)
        {
            RequirePlayer(playerId);
            if (tick < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(tick));
            }
            if (tick > _verifiedTicks[playerId])
            {
                _verifiedTicks[playerId] = tick;
            }

            int first = tick - _capacity + 1;
            if (first < 0)
            {
                first = 0;
            }
            for (int candidate = first; candidate <= tick; candidate++)
            {
                ref Slot slot = ref _slots[playerId][candidate % _capacity];
                if (slot.Command != null && slot.Tick == candidate)
                {
                    slot.Verified = true;
                }
            }
        }

        public int VerifiedTick(int playerId)
        {
            RequirePlayer(playerId);
            return _verifiedTicks[playerId];
        }

        /// <summary>True when the exact command for the tick has been confirmed by the server.</summary>
        public bool IsVerified(int playerId, int tick)
        {
            RequirePlayer(playerId);
            if (tick < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(tick));
            }

            ref Slot slot = ref _slots[playerId][tick % _capacity];
            return slot.Command != null && slot.Tick == tick && slot.Verified;
        }

        public void ClearPlayer(int playerId)
        {
            RequirePlayer(playerId);
            Array.Clear(_slots[playerId], 0, _capacity);
            _verifiedTicks[playerId] = -1;
        }

        public void Clear()
        {
            for (int i = 0; i < _maxPlayers; i++)
            {
                Array.Clear(_slots[i], 0, _capacity);
                _verifiedTicks[i] = -1;
            }
        }

        /// <summary>
        /// Serializes the commands present in [firstTick, firstTick + tickCount).
        /// Empty ticks are omitted; the receiver keeps its own repeat state.
        /// </summary>
        public int WritePacket(
            int playerId,
            int firstTick,
            int tickCount,
            byte[] buffer,
            int offset)
        {
            RequirePlayer(playerId);
            if (firstTick < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(firstTick));
            }
            if (tickCount < 0 || tickCount > _capacity)
            {
                throw new ArgumentOutOfRangeException(nameof(tickCount));
            }
            CYGWire.RequireWritable(buffer, offset, PacketHeaderSize);

            int entryCount = 0;
            for (int tick = firstTick; tick < firstTick + tickCount; tick++)
            {
                CYGCommand command;
                if (TryGetCommand(playerId, tick, out command))
                {
                    entryCount++;
                }
            }

            int written = 0;
            CYGWire.WriteByte(buffer, offset + written, PacketVersion);
            written++;
            CYGWire.WriteInt32(buffer, offset + written, playerId);
            written += 4;
            CYGWire.WriteInt32(buffer, offset + written, firstTick);
            written += 4;
            CYGWire.WriteUInt16(buffer, offset + written, (ushort)tickCount);
            written += 2;
            CYGWire.WriteUInt16(buffer, offset + written, (ushort)entryCount);
            written += 2;

            for (int tick = firstTick; tick < firstTick + tickCount; tick++)
            {
                CYGCommand command;
                if (!TryGetCommand(playerId, tick, out command))
                {
                    continue;
                }

                int payloadSize = command.PayloadSize;
                CYGWire.RequireWritable(buffer, offset + written, 10 + payloadSize);
                CYGWire.WriteInt32(buffer, offset + written, tick);
                written += 4;
                CYGWire.WriteInt32(buffer, offset + written, command.TypeId);
                written += 4;
                CYGWire.WriteUInt16(buffer, offset + written, (ushort)payloadSize);
                written += 2;
                int payload = command.WritePayload(buffer, offset + written);
                if (payload != payloadSize)
                {
                    throw new InvalidOperationException(
                        "Command " + command.GetType().Name + " declared " + payloadSize
                        + " payload bytes but wrote " + payload + ".");
                }
                written += payload;
            }

            return written;
        }

        /// <summary>Reads a packet written by <see cref="WritePacket"/> and returns the entry count.</summary>
        public int ReadPacket(byte[] buffer, int offset, int length, out int playerId, out int firstTick)
        {
            CYGWire.RequireReadable(buffer, offset, PacketHeaderSize);
            if (length < PacketHeaderSize)
            {
                throw new InvalidOperationException("Command packet is truncated.");
            }

            int read = 0;
            byte version = CYGWire.ReadByte(buffer, offset + read);
            read++;
            if (version != PacketVersion)
            {
                throw new InvalidOperationException("Unsupported command packet version " + version + ".");
            }

            playerId = CYGWire.ReadInt32(buffer, offset + read);
            read += 4;
            RequirePlayer(playerId);
            firstTick = CYGWire.ReadInt32(buffer, offset + read);
            read += 4;
            if (firstTick < 0)
            {
                throw new InvalidOperationException("Command packet has a negative first tick.");
            }

            int tickCount = CYGWire.ReadUInt16(buffer, offset + read);
            read += 2;
            int entryCount = CYGWire.ReadUInt16(buffer, offset + read);
            read += 2;
            if (tickCount > _capacity || entryCount > tickCount)
            {
                throw new InvalidOperationException("Command packet header is out of range.");
            }

            for (int i = 0; i < entryCount; i++)
            {
                if (length - read < 10)
                {
                    throw new InvalidOperationException("Command packet entry is truncated.");
                }

                int tick = CYGWire.ReadInt32(buffer, offset + read);
                read += 4;
                int typeId = CYGWire.ReadInt32(buffer, offset + read);
                read += 4;
                int payloadSize = CYGWire.ReadUInt16(buffer, offset + read);
                read += 2;
                if (tick < firstTick || tick >= firstTick + tickCount
                    || payloadSize > CYGCommand.MaxPayloadSize
                    || length - read < payloadSize)
                {
                    throw new InvalidOperationException("Command packet entry is invalid.");
                }

                Func<CYGCommand> factory;
                if (!_factories.TryGetValue(typeId, out factory))
                {
                    throw new InvalidOperationException("No factory registered for command TypeId " + typeId + ".");
                }

                CYGCommand command = factory();
                if (command == null || command.TypeId != typeId)
                {
                    throw new InvalidOperationException(
                        "Factory for command TypeId " + typeId + " produced an invalid command.");
                }
                command.ReadPayload(buffer, offset + read, payloadSize);
                read += payloadSize;
                AddCommand(playerId, tick, command);
            }

            return entryCount;
        }

        private void RequirePlayer(int playerId)
        {
            if ((uint)playerId >= (uint)_maxPlayers)
            {
                throw new ArgumentOutOfRangeException(nameof(playerId));
            }
        }

        private static void FactoryMustProduce(int typeId, Func<CYGCommand> factory)
        {
            CYGCommand probe = factory();
            if (probe == null || probe.TypeId != typeId)
            {
                throw new ArgumentException(
                    "Factory must create a command with TypeId " + typeId + ".", nameof(factory));
            }
        }
    }
}
