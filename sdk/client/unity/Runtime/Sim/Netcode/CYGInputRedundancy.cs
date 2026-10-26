using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Redundant input packet codec. Each packet repeats the last N inputs for
    /// a player so a single lost packet does not stall confirmation. Duplicated
    /// inputs are idempotent: replaying a confirmed tick overwrites it with the
    /// same values.
    /// </summary>
    public static class CYGInputRedundancy
    {
        public const byte PacketVersion = 1;
        public const int MaxInputs = 32;
        public const int HeaderSize = 6;

        public static int WritePacket(
            byte[] buffer,
            int offset,
            int playerId,
            CYGPlayerInput[] inputs,
            int count)
        {
            if (playerId < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(playerId));
            }
            if (inputs == null)
            {
                throw new ArgumentNullException(nameof(inputs));
            }
            if (count < 1 || count > MaxInputs || count > inputs.Length)
            {
                throw new ArgumentOutOfRangeException(nameof(count));
            }

            int size = HeaderSize + count * CYGPlayerInput.SizeInBytes;
            CYGWire.RequireWritable(buffer, offset, size);

            CYGWire.WriteByte(buffer, offset, PacketVersion);
            CYGWire.WriteInt32(buffer, offset + 1, playerId);
            CYGWire.WriteByte(buffer, offset + 5, (byte)count);

            int written = HeaderSize;
            for (int i = 0; i < count; i++)
            {
                inputs[i].WriteTo(buffer, offset + written);
                written += CYGPlayerInput.SizeInBytes;
            }

            return written;
        }

        /// <summary>Decodes a packet without storing anything. Ticks must be strictly increasing.</summary>
        public static bool TryReadPacket(
            byte[] buffer,
            int offset,
            int length,
            CYGPlayerInput[] output,
            out int playerId,
            out int count)
        {
            playerId = -1;
            count = 0;
            if (buffer == null || output == null)
            {
                return false;
            }
            try
            {
                CYGWire.RequireReadable(buffer, offset, length);
            }
            catch (ArgumentOutOfRangeException)
            {
                return false;
            }

            if (length < HeaderSize || CYGWire.ReadByte(buffer, offset) != PacketVersion)
            {
                return false;
            }

            int id = CYGWire.ReadInt32(buffer, offset + 1);
            int inputCount = CYGWire.ReadByte(buffer, offset + 5);
            if (id < 0 || inputCount < 1 || inputCount > MaxInputs || inputCount > output.Length)
            {
                return false;
            }

            int expected = HeaderSize + inputCount * CYGPlayerInput.SizeInBytes;
            if (length < expected)
            {
                return false;
            }

            int previousTick = -1;
            for (int i = 0; i < inputCount; i++)
            {
                CYGPlayerInput input = CYGPlayerInput.ReadFrom(
                    buffer, offset + HeaderSize + i * CYGPlayerInput.SizeInBytes).Clamped();
                if (previousTick >= 0 && input.Tick != previousTick + 1)
                {
                    return false;
                }
                previousTick = input.Tick;
                output[i] = input;
            }

            playerId = id;
            count = inputCount;
            return true;
        }

        /// <summary>
        /// Decodes a packet and writes every input into the ring as confirmed.
        /// Returns the number of inputs applied, or 0 for a rejected packet.
        /// </summary>
        public static int ReceiveInto(
            byte[] buffer,
            int offset,
            int length,
            CYGPlayerInput[] scratch,
            CYGInputRingBuffer ring,
            out int playerId)
        {
            playerId = -1;
            if (scratch == null)
            {
                throw new ArgumentNullException(nameof(scratch));
            }
            if (ring == null)
            {
                throw new ArgumentNullException(nameof(ring));
            }

            int count;
            if (!TryReadPacket(buffer, offset, length, scratch, out playerId, out count))
            {
                return 0;
            }

            for (int i = 0; i < count; i++)
            {
                ring.SetInput(playerId, scratch[i].Tick, scratch[i], isPredicted: false);
            }
            return count;
        }
    }
}
