using System;

namespace CookYourGame.Sim.Headless
{
    public enum CYGMatchMessage : byte
    {
        Input = 1,
        RelayInput = 2,
        ServerStatus = 3,
        FinalReport = 4,
    }

    /// <summary>
    /// Wire format for the headless match. Input messages carry the same
    /// redundancy body used by opcode 19, prefixed with a message type so the
    /// match can be driven without the Unity transport.
    /// </summary>
    public static class CYGMatchProtocol
    {
        public const int HeaderSize = 1;
        public const int InputHeaderSize = 5;
        public const int StatusSize = 10;
        public const int ReportHeaderSize = 5;

        public static int WriteInput(
            byte[] buffer,
            int playerId,
            CYGPlayerInput[] inputs,
            int count,
            int requestedPeerTick)
        {
            CYGWire.WriteByte(buffer, 0, (byte)CYGMatchMessage.Input);
            CYGWire.WriteInt32(buffer, 1, requestedPeerTick);
            return InputHeaderSize + CYGInputRedundancy.WritePacket(
                buffer, InputHeaderSize, playerId, inputs, count);
        }

        public static int WriteRelay(byte[] buffer, byte[] source, int sourceOffset, int sourceLength)
        {
            if (buffer == null)
            {
                throw new ArgumentNullException(nameof(buffer));
            }
            if (source == null)
            {
                throw new ArgumentNullException(nameof(source));
            }
            CYGWire.RequireReadable(source, sourceOffset, sourceLength);
            CYGWire.RequireWritable(buffer, 0, sourceLength + HeaderSize);

            CYGWire.WriteByte(buffer, 0, (byte)CYGMatchMessage.RelayInput);
            Array.Copy(source, sourceOffset, buffer, HeaderSize, sourceLength);
            return sourceLength + HeaderSize;
        }

        public static int WriteStatus(
            byte[] buffer,
            int serverFrame,
            int missingPlayerId,
            int missingTick)
        {
            CYGWire.WriteByte(buffer, 0, (byte)CYGMatchMessage.ServerStatus);
            CYGWire.WriteInt32(buffer, 1, serverFrame);
            CYGWire.WriteByte(buffer, 5, (byte)missingPlayerId);
            CYGWire.WriteInt32(buffer, 6, missingTick);
            return StatusSize;
        }

        public static bool TryReadStatus(
            byte[] buffer,
            int offset,
            int length,
            out int serverFrame,
            out int missingPlayerId,
            out int missingTick)
        {
            serverFrame = 0;
            missingPlayerId = 0;
            missingTick = -1;
            if (buffer == null || length < StatusSize)
            {
                return false;
            }
            if (buffer[offset] != (byte)CYGMatchMessage.ServerStatus)
            {
                return false;
            }
            serverFrame = CYGWire.ReadInt32(buffer, offset + 1);
            missingPlayerId = CYGWire.ReadByte(buffer, offset + 5);
            missingTick = CYGWire.ReadInt32(buffer, offset + 6);
            return serverFrame >= 0;
        }

        public static int WriteReport(byte[] buffer, uint[] checksums, int count)
        {
            if (checksums == null || count < 0 || count > checksums.Length)
            {
                throw new ArgumentOutOfRangeException(nameof(count));
            }
            int size = ReportHeaderSize + count * 4;
            CYGWire.RequireWritable(buffer, 0, size);

            CYGWire.WriteByte(buffer, 0, (byte)CYGMatchMessage.FinalReport);
            CYGWire.WriteInt32(buffer, 1, count);
            for (int i = 0; i < count; i++)
            {
                CYGWire.WriteUInt32(buffer, ReportHeaderSize + i * 4, checksums[i]);
            }
            return size;
        }

        public static bool TryReadReport(
            byte[] buffer,
            int offset,
            int length,
            uint[] output,
            out int count)
        {
            count = 0;
            if (buffer == null || output == null || length < ReportHeaderSize)
            {
                return false;
            }
            if (buffer[offset] != (byte)CYGMatchMessage.FinalReport)
            {
                return false;
            }

            int reported = CYGWire.ReadInt32(buffer, offset + 1);
            if (reported < 0 || reported > output.Length)
            {
                return false;
            }
            if (length < ReportHeaderSize + reported * 4)
            {
                return false;
            }

            for (int i = 0; i < reported; i++)
            {
                output[i] = CYGWire.ReadUInt32(buffer, offset + ReportHeaderSize + i * 4);
            }
            count = reported;
            return true;
        }
    }
}
