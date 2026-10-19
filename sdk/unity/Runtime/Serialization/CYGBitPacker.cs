using System;
using System.Buffers;
using System.Runtime.InteropServices;
using System.Text;
using UnityEngine;

namespace CookYourGame
{
    /// <summary>Reinterprets a float as its raw IEEE754 bits without unsafe code.</summary>
    [StructLayout(LayoutKind.Explicit)]
    internal struct CYGFloatBits
    {
        [FieldOffset(0)] public float value;
        [FieldOffset(0)] public uint bits;
    }

    /// <summary>Byte-aligned, varint-based packet writer backed by a pooled, growable buffer.</summary>
    public sealed class CYGBitWriter
    {
        // Every value in this writer is byte aligned; "bit" packing happens inside fixed-width fields.
        const int MinCapacity = 64;

        static CYGFloatBits bitsUnion;

        byte[] buffer;
        int position;

        /// <summary>Creates a writer with the given initial byte capacity.</summary>
        public CYGBitWriter(int initialCapacity = 256)
        {
            if (initialCapacity < MinCapacity)
            {
                initialCapacity = MinCapacity;
            }

            buffer = ArrayPool<byte>.Shared.Rent(initialCapacity);
            position = 0;
        }

        /// <summary>Number of bytes written since the last Reset.</summary>
        public int Length
        {
            get { return position; }
        }

        /// <summary>Discards the written content without releasing the pooled buffer.</summary>
        public void Reset()
        {
            position = 0;
        }

        /// <summary>Live view over the written bytes; valid until the next write grows the buffer.</summary>
        public ArraySegment<byte> ToArraySegment()
        {
            return new ArraySegment<byte>(buffer, 0, position);
        }

        /// <summary>Writes a single raw byte.</summary>
        public void WriteByte(byte value)
        {
            EnsureCapacity(1);
            buffer[position++] = value;
        }

        /// <summary>Writes a boolean as one byte (0 or 1).</summary>
        public void WriteBool(bool value)
        {
            WriteByte(value ? (byte)1 : (byte)0);
        }

        /// <summary>Writes an unsigned 32-bit integer as an LEB128 varint.</summary>
        public void WriteUInt32(uint value)
        {
            EnsureCapacity(5);
            while (value >= 0x80u)
            {
                buffer[position++] = (byte)(value | 0x80u);
                value >>= 7;
            }

            buffer[position++] = (byte)value;
        }

        /// <summary>Writes a signed 32-bit integer as zig-zag encoded LEB128.</summary>
        public void WriteInt32(int value)
        {
            WriteUInt32((uint)((value << 1) ^ (value >> 31)));
        }

        /// <summary>Writes an unsigned 64-bit integer as an LEB128 varint.</summary>
        public void WriteUInt64(ulong value)
        {
            EnsureCapacity(10);
            while (value >= 0x80UL)
            {
                buffer[position++] = (byte)(value | 0x80UL);
                value >>= 7;
            }

            buffer[position++] = (byte)value;
        }

        /// <summary>Writes a signed 64-bit integer as zig-zag encoded LEB128.</summary>
        public void WriteInt64(long value)
        {
            WriteUInt64((ulong)((value << 1) ^ (value >> 63)));
        }

        /// <summary>Writes a 4-byte big-endian IEEE754 single.</summary>
        public void WriteSingle(float value)
        {
            EnsureCapacity(4);
            bitsUnion.value = value;
            uint raw = bitsUnion.bits;
            buffer[position++] = (byte)(raw >> 24);
            buffer[position++] = (byte)(raw >> 16);
            buffer[position++] = (byte)(raw >> 8);
            buffer[position++] = (byte)raw;
        }

        /// <summary>Writes a 2-byte IEEE754 half-precision float.</summary>
        public void WriteHalf(float value)
        {
            EnsureCapacity(2);
            ushort half = ToHalf(value);
            buffer[position++] = (byte)(half >> 8);
            buffer[position++] = (byte)half;
        }

        /// <summary>Writes a vector as three full-precision floats.</summary>
        public void WriteVector3(Vector3 v)
        {
            WriteSingle(v.x);
            WriteSingle(v.y);
            WriteSingle(v.z);
        }

        /// <summary>Writes a vector as three big-endian int16 fixed-point components at 1 unit per 100.</summary>
        public void WriteVector3Packed(Vector3 v)
        {
            EnsureCapacity(6);
            position = WritePackedAxis(buffer, position, v.x);
            position = WritePackedAxis(buffer, position, v.y);
            position = WritePackedAxis(buffer, position, v.z);
        }

        /// <summary>Writes a quaternion as the smallest-three encoding in exactly 4 bytes.</summary>
        public void WriteQuaternion(Quaternion q)
        {
            EnsureCapacity(4);

            float x = q.x, y = q.y, z = q.z, w = q.w;

            float ax = Math.Abs(x), ay = Math.Abs(y), az = Math.Abs(z), aw = Math.Abs(w);
            int largest = 0;
            float max = ax;
            if (ay > max) { largest = 1; max = ay; }
            if (az > max) { largest = 2; max = az; }
            if (aw > max) { largest = 3; max = aw; }

            uint packed = (uint)largest << 30;
            int index = 0;
            for (int i = 0; i < 4; i++)
            {
                if (i == largest)
                {
                    continue;
                }

                float component = i == 0 ? x : (i == 1 ? y : (i == 2 ? z : w));
                packed |= (uint)QuantizeUnit(component) << (20 - index * 10);
                index++;
            }

            buffer[position++] = (byte)(packed >> 24);
            buffer[position++] = (byte)(packed >> 16);
            buffer[position++] = (byte)(packed >> 8);
            buffer[position++] = (byte)packed;
        }

        /// <summary>Writes a quaternion as four full-precision floats.</summary>
        public void WriteQuaternionRaw(Quaternion q)
        {
            WriteSingle(q.x);
            WriteSingle(q.y);
            WriteSingle(q.z);
            WriteSingle(q.w);
        }

        /// <summary>Writes a string as an LEB128 byte length followed by its UTF8 bytes.</summary>
        public void WriteString(string s)
        {
            if (string.IsNullOrEmpty(s))
            {
                WriteUInt32(0);
                return;
            }

            int byteCount = Encoding.UTF8.GetByteCount(s);
            WriteUInt32((uint)byteCount);
            EnsureCapacity(byteCount);
            int written = Encoding.UTF8.GetBytes(s, 0, s.Length, buffer, position);
            position += written;
        }

        /// <summary>Writes raw bytes as an LEB128 length followed by the payload.</summary>
        public void WriteBytes(ArraySegment<byte> data)
        {
            WriteUInt32((uint)data.Count);
            EnsureCapacity(data.Count);
            if (data.Count > 0)
            {
                Buffer.BlockCopy(data.Array, data.Offset, buffer, position, data.Count);
                position += data.Count;
            }
        }

        /// <summary>Appends bytes verbatim, without a length prefix.</summary>
        public void WriteRaw(ArraySegment<byte> data)
        {
            EnsureCapacity(data.Count);
            if (data.Count > 0)
            {
                Buffer.BlockCopy(data.Array, data.Offset, buffer, position, data.Count);
                position += data.Count;
            }
        }

        /// <summary>Writes a 16-bit value big-endian, used for fixed-width ids.</summary>
        public void WriteUInt16BigEndian(ushort value)
        {
            EnsureCapacity(2);
            buffer[position++] = (byte)(value >> 8);
            buffer[position++] = (byte)value;
        }

        void EnsureCapacity(int extra)
        {
            int needed = position + extra;
            if (needed <= buffer.Length)
            {
                return;
            }

            int capacity = buffer.Length;
            while (capacity < needed)
            {
                capacity <<= 1;
            }

            byte[] grown = ArrayPool<byte>.Shared.Rent(capacity);
            Buffer.BlockCopy(buffer, 0, grown, 0, position);
            ArrayPool<byte>.Shared.Return(buffer);
            buffer = grown;
        }

        static int WritePackedAxis(byte[] target, int at, float value)
        {
            int scaled = ClampToShort(value * 100f);
            target[at++] = (byte)((scaled >> 8) & 0xFF);
            target[at++] = (byte)(scaled & 0xFF);
            return at;
        }

        static short ClampToShort(float value)
        {
            if (float.IsNaN(value))
            {
                return 0;
            }

            float scaled = (float)Math.Round(value, MidpointRounding.AwayFromZero);
            if (scaled > short.MaxValue)
            {
                return short.MaxValue;
            }

            if (scaled < short.MinValue)
            {
                return short.MinValue;
            }

            return (short)scaled;
        }

        static int QuantizeUnit(float value)
        {
            if (float.IsNaN(value))
            {
                return 512;
            }

            int encoded = (int)Math.Round(value * 512f, MidpointRounding.AwayFromZero) + 512;
            if (encoded < 0)
            {
                return 0;
            }

            if (encoded > 1023)
            {
                return 1023;
            }

            return encoded;
        }

        static float DequantizeUnit(int encoded)
        {
            return (encoded - 512) / 512f;
        }

        internal static ushort ToHalf(float value)
        {
            if (float.IsNaN(value))
            {
                return 0x7FE0;
            }

            bitsUnion.value = value;
            uint raw = bitsUnion.bits;
            ushort sign = (ushort)((raw >> 16) & 0x8000u);
            int exponent = (int)((raw >> 23) & 0xFF) - 127;
            uint mantissa = raw & 0x7FFFFFu;

            if (exponent == 128)
            {
                return (ushort)(sign | 0x7C00u | (mantissa != 0 ? 0x200u : 0u));
            }

            int halfExponent = exponent + 15;
            if (halfExponent >= 31)
            {
                return (ushort)(sign | 0x7C00u);
            }

            if (halfExponent <= 0)
            {
                if (halfExponent < -10)
                {
                    return sign;
                }

                mantissa |= 0x800000u;
                int shift = 14 - halfExponent;
                uint truncated = mantissa >> shift;
                uint remainder = mantissa & ((1u << shift) - 1u);
                if (remainder > (1u << (shift - 1)))
                {
                    truncated++;
                }

                return (ushort)(sign | truncated);
            }

            uint halfMantissa = mantissa >> 13;
            uint result = ((uint)halfExponent << 10) | halfMantissa;
            uint dropped = mantissa & 0x1FFFu;
            if (dropped > 0x1000u || (dropped == 0x1000u && (halfMantissa & 1u) != 0))
            {
                result++;
            }

            return (ushort)(sign | result);
        }

        internal static float FromHalf(ushort half)
        {
            uint sign = (uint)(half & 0x8000) << 16;
            int exponent = (half >> 10) & 0x1F;
            int mantissa = half & 0x3FF;
            uint raw;

            if (exponent == 0)
            {
                if (mantissa == 0)
                {
                    raw = sign;
                }
                else
                {
                    exponent = 1;
                    while ((mantissa & 0x400) == 0)
                    {
                        mantissa <<= 1;
                        exponent--;
                    }

                    mantissa &= 0x3FF;
                    raw = sign | ((uint)(exponent + 127 - 15) << 23) | ((uint)mantissa << 13);
                }
            }
            else if (exponent == 31)
            {
                raw = sign | 0x7F800000u | ((uint)mantissa << 13);
            }
            else
            {
                raw = sign | ((uint)(exponent + 127 - 15) << 23) | ((uint)mantissa << 13);
            }

            bitsUnion.bits = raw;
            return bitsUnion.value;
        }

        static int DequantizeComponent(uint packed, int slot)
        {
            return (int)((packed >> (20 - slot * 10)) & 0x3FFu);
        }

        internal static Quaternion FromPacked(uint packed)
        {
            int largest = (int)(packed >> 30) & 0x3;
            float a = DequantizeUnit(DequantizeComponent(packed, 0));
            float b = DequantizeUnit(DequantizeComponent(packed, 1));
            float c = DequantizeUnit(DequantizeComponent(packed, 2));

            float sum = a * a + b * b + c * c;
            float remainder = sum < 1f ? (float)Math.Sqrt(1f - sum) : 0f;

            float x, y, z, w;

            // The largest component's sign is not transmitted: q and -q are the same rotation.
            switch (largest)
            {
                case 0: x = remainder; y = a; z = b; w = c; break;
                case 1: x = a; y = remainder; z = b; w = c; break;
                case 2: x = a; y = b; z = remainder; w = c; break;
                default: x = a; y = b; z = c; w = remainder; break;
            }

            return new Quaternion(x, y, z, w);
        }
    }

    /// <summary>Zero-copy packet reader over a caller-owned buffer slice.</summary>
    public sealed class CYGBitReader
    {
        static CYGFloatBits bitsUnion;

        byte[] buffer;
        int offset;
        int length;
        int position;

        /// <summary>Creates an unattached reader; call Attach before reading.</summary>
        public CYGBitReader()
        {
            buffer = Array.Empty<byte>();
        }

        /// <summary>Points the reader at a slice; the caller keeps ownership of the bytes.</summary>
        public void Attach(byte[] buffer, int offset, int length)
        {
            if (buffer == null)
            {
                Debug.LogError("[CYG] CYGBitReader.Attach received a null buffer.");
                this.buffer = Array.Empty<byte>();
                this.offset = 0;
                this.length = 0;
                position = 0;
                return;
            }

            if (offset < 0 || length < 0 || offset + length > buffer.Length)
            {
                Debug.LogError("[CYG] CYGBitReader.Attach received an out-of-range slice.");
                this.buffer = Array.Empty<byte>();
                this.offset = 0;
                this.length = 0;
                position = 0;
                return;
            }

            this.buffer = buffer;
            this.offset = offset;
            this.length = length;
            position = 0;
        }

        /// <summary>Bytes left to read in the attached slice.</summary>
        public int Remaining
        {
            get { return length - position; }
        }

        /// <summary>Bytes consumed from the attached slice since Attach.</summary>
        public int BytesRead
        {
            get { return position; }
        }

        /// <summary>Reads a single raw byte.</summary>
        public byte ReadByte()
        {
            Require(1);
            return buffer[offset + position++];
        }

        /// <summary>Reads a boolean written as one byte.</summary>
        public bool ReadBool()
        {
            return ReadByte() != 0;
        }

        /// <summary>Reads an LEB128 unsigned 32-bit varint.</summary>
        public uint ReadUInt32()
        {
            uint result = 0;
            int shift = 0;
            while (true)
            {
                Require(1);
                byte current = buffer[offset + position++];
                result |= (uint)(current & 0x7F) << shift;
                if ((current & 0x80) == 0)
                {
                    return result;
                }

                shift += 7;
                if (shift > 31)
                {
                    throw new InvalidOperationException("[CYG] CYGBitReader: malformed uint32 varint.");
                }
            }
        }

        /// <summary>Reads a zig-zag encoded signed 32-bit varint.</summary>
        public int ReadInt32()
        {
            uint raw = ReadUInt32();
            return (int)((raw >> 1) ^ (uint)(-(int)(raw & 1u)));
        }

        /// <summary>Reads an LEB128 unsigned 64-bit varint.</summary>
        public ulong ReadUInt64()
        {
            ulong result = 0;
            int shift = 0;
            while (true)
            {
                Require(1);
                byte current = buffer[offset + position++];
                result |= (ulong)(current & 0x7F) << shift;
                if ((current & 0x80) == 0)
                {
                    return result;
                }

                shift += 7;
                if (shift > 63)
                {
                    throw new InvalidOperationException("[CYG] CYGBitReader: malformed uint64 varint.");
                }
            }
        }

        /// <summary>Reads a zig-zag encoded signed 64-bit varint.</summary>
        public long ReadInt64()
        {
            ulong raw = ReadUInt64();
            return (long)((raw >> 1) ^ (ulong)-(long)(raw & 1UL));
        }

        /// <summary>Reads a 4-byte big-endian IEEE754 single.</summary>
        public float ReadSingle()
        {
            Require(4);
            uint raw = (uint)(
                (buffer[offset + position] << 24) |
                (buffer[offset + position + 1] << 16) |
                (buffer[offset + position + 2] << 8) |
                buffer[offset + position + 3]);
            position += 4;
            bitsUnion.bits = raw;
            return bitsUnion.value;
        }

        /// <summary>Reads a 2-byte IEEE754 half-precision float.</summary>
        public float ReadHalf()
        {
            Require(2);
            ushort raw = (ushort)((buffer[offset + position] << 8) | buffer[offset + position + 1]);
            position += 2;
            return CYGBitWriter.FromHalf(raw);
        }

        /// <summary>Reads a vector of three full-precision floats.</summary>
        public Vector3 ReadVector3()
        {
            float x = ReadSingle();
            float y = ReadSingle();
            float z = ReadSingle();
            return new Vector3(x, y, z);
        }

        /// <summary>Reads a vector of three int16 fixed-point components at 1 unit per 100.</summary>
        public Vector3 ReadVector3Packed()
        {
            Require(6);
            float x = ReadPackedAxis(position);
            float y = ReadPackedAxis(position + 2);
            float z = ReadPackedAxis(position + 4);
            position += 6;
            return new Vector3(x, y, z);
        }

        /// <summary>Reads the 4-byte smallest-three quaternion encoding.</summary>
        public Quaternion ReadQuaternion()
        {
            Require(4);
            uint packed = (uint)(
                (buffer[offset + position] << 24) |
                (buffer[offset + position + 1] << 16) |
                (buffer[offset + position + 2] << 8) |
                buffer[offset + position + 3]);
            position += 4;
            return CYGBitWriter.FromPacked(packed);
        }

        /// <summary>Reads a quaternion written as four full-precision floats.</summary>
        public Quaternion ReadQuaternionRaw()
        {
            float x = ReadSingle();
            float y = ReadSingle();
            float z = ReadSingle();
            float w = ReadSingle();
            return new Quaternion(x, y, z, w);
        }

        /// <summary>Reads an LEB128 length-prefixed UTF8 string.</summary>
        public string ReadString()
        {
            int count = (int)ReadUInt32();
            if (count == 0)
            {
                return string.Empty;
            }

            Require(count);
            string result = Encoding.UTF8.GetString(buffer, offset + position, count);
            position += count;
            return result;
        }

        /// <summary>Reads a raw payload slice; valid only while the attached buffer is alive.</summary>
        public ArraySegment<byte> ReadBytes(int count)
        {
            Require(count);
            ArraySegment<byte> result = new ArraySegment<byte>(buffer, offset + position, count);
            position += count;
            return result;
        }

        /// <summary>Reads a 16-bit big-endian value used for fixed-width ids.</summary>
        public ushort ReadUInt16BigEndian()
        {
            Require(2);
            ushort result = (ushort)((buffer[offset + position] << 8) | buffer[offset + position + 1]);
            position += 2;
            return result;
        }

        float ReadPackedAxis(int at)
        {
            short raw = (short)((buffer[offset + at] << 8) | buffer[offset + at + 1]);
            return raw / 100f;
        }

        void Require(int count)
        {
            if (position + count > length)
            {
                throw new InvalidOperationException(
                    "[CYG] CYGBitReader underflow: needed " + count + " bytes, " + (length - position) + " available.");
            }
        }
    }
}
