using System;
using System.Runtime.InteropServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Explicit little-endian wire primitives. The generated component code uses
    /// these instead of raw struct memory so padding and host endianness cannot
    /// leak into snapshots or checksums.
    /// </summary>
    public static class CYGWire
    {
        [StructLayout(LayoutKind.Explicit)]
        private struct SingleInt
        {
            [FieldOffset(0)] public float Single;
            [FieldOffset(0)] public int Int;
        }

        [StructLayout(LayoutKind.Explicit)]
        private struct DoubleLong
        {
            [FieldOffset(0)] public double Double;
            [FieldOffset(0)] public long Long;
        }

        public static void RequireWritable(byte[] buffer, int offset, int size)
        {
            if (buffer == null)
            {
                throw new ArgumentNullException(nameof(buffer));
            }
            if (offset < 0 || size < 0 || offset > buffer.Length || size > buffer.Length - offset)
            {
                throw new ArgumentOutOfRangeException(nameof(offset));
            }
        }

        public static void RequireReadable(byte[] buffer, int offset, int size)
        {
            if (buffer == null)
            {
                throw new ArgumentNullException(nameof(buffer));
            }
            if (offset < 0 || size < 0 || offset > buffer.Length || size > buffer.Length - offset)
            {
                throw new ArgumentOutOfRangeException(nameof(offset));
            }
        }

        public static void WriteByte(byte[] buffer, int offset, byte value)
        {
            buffer[offset] = value;
        }

        public static byte ReadByte(byte[] buffer, int offset)
        {
            return buffer[offset];
        }

        public static void WriteSByte(byte[] buffer, int offset, sbyte value)
        {
            buffer[offset] = unchecked((byte)value);
        }

        public static sbyte ReadSByte(byte[] buffer, int offset)
        {
            return unchecked((sbyte)buffer[offset]);
        }

        public static void WriteInt16(byte[] buffer, int offset, short value)
        {
            buffer[offset] = (byte)value;
            buffer[offset + 1] = (byte)(value >> 8);
        }

        public static short ReadInt16(byte[] buffer, int offset)
        {
            return unchecked((short)(buffer[offset] | (buffer[offset + 1] << 8)));
        }

        public static void WriteUInt16(byte[] buffer, int offset, ushort value)
        {
            buffer[offset] = (byte)value;
            buffer[offset + 1] = (byte)(value >> 8);
        }

        public static ushort ReadUInt16(byte[] buffer, int offset)
        {
            return (ushort)(buffer[offset] | (buffer[offset + 1] << 8));
        }

        public static void WriteInt32(byte[] buffer, int offset, int value)
        {
            buffer[offset] = (byte)value;
            buffer[offset + 1] = (byte)(value >> 8);
            buffer[offset + 2] = (byte)(value >> 16);
            buffer[offset + 3] = (byte)(value >> 24);
        }

        public static int ReadInt32(byte[] buffer, int offset)
        {
            return buffer[offset]
                | (buffer[offset + 1] << 8)
                | (buffer[offset + 2] << 16)
                | (buffer[offset + 3] << 24);
        }

        public static void WriteUInt32(byte[] buffer, int offset, uint value)
        {
            WriteInt32(buffer, offset, unchecked((int)value));
        }

        public static uint ReadUInt32(byte[] buffer, int offset)
        {
            return unchecked((uint)ReadInt32(buffer, offset));
        }

        public static void WriteInt64(byte[] buffer, int offset, long value)
        {
            buffer[offset] = (byte)value;
            buffer[offset + 1] = (byte)(value >> 8);
            buffer[offset + 2] = (byte)(value >> 16);
            buffer[offset + 3] = (byte)(value >> 24);
            buffer[offset + 4] = (byte)(value >> 32);
            buffer[offset + 5] = (byte)(value >> 40);
            buffer[offset + 6] = (byte)(value >> 48);
            buffer[offset + 7] = (byte)(value >> 56);
        }

        public static long ReadInt64(byte[] buffer, int offset)
        {
            return (long)(uint)(
                buffer[offset]
                | (buffer[offset + 1] << 8)
                | (buffer[offset + 2] << 16)
                | (buffer[offset + 3] << 24))
                | ((long)(uint)(
                    buffer[offset + 4]
                    | (buffer[offset + 5] << 8)
                    | (buffer[offset + 6] << 16)
                    | (buffer[offset + 7] << 24)) << 32);
        }

        public static void WriteUInt64(byte[] buffer, int offset, ulong value)
        {
            WriteInt64(buffer, offset, unchecked((long)value));
        }

        public static ulong ReadUInt64(byte[] buffer, int offset)
        {
            return unchecked((ulong)ReadInt64(buffer, offset));
        }

        public static void WriteSingle(byte[] buffer, int offset, float value)
        {
            SingleInt bits = new SingleInt();
            bits.Single = value;
            WriteInt32(buffer, offset, bits.Int);
        }

        public static float ReadSingle(byte[] buffer, int offset)
        {
            SingleInt bits = new SingleInt();
            bits.Int = ReadInt32(buffer, offset);
            return bits.Single;
        }

        public static void WriteDouble(byte[] buffer, int offset, double value)
        {
            DoubleLong bits = new DoubleLong();
            bits.Double = value;
            WriteInt64(buffer, offset, bits.Long);
        }

        public static double ReadDouble(byte[] buffer, int offset)
        {
            DoubleLong bits = new DoubleLong();
            bits.Long = ReadInt64(buffer, offset);
            return bits.Double;
        }
    }
}
