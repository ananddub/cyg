using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// High-performance, zero-allocation CRC32 and FNV-1a checksum calculation for deterministic world states.
    /// Used for frame-by-frame synchronization checks and instant desync detection between client and server.
    /// </summary>
    public static class CYGChecksum
    {
        private static readonly uint[] CrcTable = InitializeCrcTable();

        private static uint[] InitializeCrcTable()
        {
            const uint polynomial = 0xEDB88320u; // IEEE 802.3
            uint[] table = new uint[256];
            for (uint i = 0; i < 256; i++)
            {
                uint entry = i;
                for (int j = 0; j < 8; j++)
                {
                    if ((entry & 1) == 1)
                        entry = (entry >> 1) ^ polynomial;
                    else
                        entry >>= 1;
                }
                table[i] = entry;
            }
            return table;
        }

        #region Core Hash Steps (Zero Allocation)

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private static uint UpdateCrc(uint crc, byte b)
        {
            return CrcTable[(crc ^ b) & 0xFF] ^ (crc >> 8);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private static uint UpdateCrc(uint crc, int value)
        {
            crc = CrcTable[(crc ^ (byte)(value & 0xFF)) & 0xFF] ^ (crc >> 8);
            crc = CrcTable[(crc ^ (byte)((value >> 8) & 0xFF)) & 0xFF] ^ (crc >> 8);
            crc = CrcTable[(crc ^ (byte)((value >> 16) & 0xFF)) & 0xFF] ^ (crc >> 8);
            crc = CrcTable[(crc ^ (byte)((value >> 24) & 0xFF)) & 0xFF] ^ (crc >> 8);
            return crc;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private static uint UpdateCrc(uint crc, long value)
        {
            crc = CrcTable[(crc ^ (byte)(value & 0xFF)) & 0xFF] ^ (crc >> 8);
            crc = CrcTable[(crc ^ (byte)((value >> 8) & 0xFF)) & 0xFF] ^ (crc >> 8);
            crc = CrcTable[(crc ^ (byte)((value >> 16) & 0xFF)) & 0xFF] ^ (crc >> 8);
            crc = CrcTable[(crc ^ (byte)((value >> 24) & 0xFF)) & 0xFF] ^ (crc >> 8);
            crc = CrcTable[(crc ^ (byte)((value >> 32) & 0xFF)) & 0xFF] ^ (crc >> 8);
            crc = CrcTable[(crc ^ (byte)((value >> 40) & 0xFF)) & 0xFF] ^ (crc >> 8);
            crc = CrcTable[(crc ^ (byte)((value >> 48) & 0xFF)) & 0xFF] ^ (crc >> 8);
            crc = CrcTable[(crc ^ (byte)((value >> 56) & 0xFF)) & 0xFF] ^ (crc >> 8);
            return crc;
        }

        #endregion

        #region Generated Component Hashing

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static void HashByte(ref uint crc, byte value)
        {
            crc = UpdateCrc(crc, value);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static void HashInt16(ref uint crc, short value)
        {
            crc = UpdateCrc(crc, (byte)value);
            crc = UpdateCrc(crc, (byte)(value >> 8));
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static void HashUInt16(ref uint crc, ushort value)
        {
            crc = UpdateCrc(crc, (byte)value);
            crc = UpdateCrc(crc, (byte)(value >> 8));
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static void HashInt(ref uint crc, int value)
        {
            crc = UpdateCrc(crc, value);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static void HashUInt(ref uint crc, uint value)
        {
            crc = UpdateCrc(crc, unchecked((int)value));
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static void HashLong(ref uint crc, long value)
        {
            crc = UpdateCrc(crc, value);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static void HashULong(ref uint crc, ulong value)
        {
            crc = UpdateCrc(crc, unchecked((long)value));
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static void HashSingle(ref uint crc, float value)
        {
            SingleInt bits = new SingleInt();
            bits.Single = value;
            crc = UpdateCrc(crc, bits.Int);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static void HashDouble(ref uint crc, double value)
        {
            DoubleLong bits = new DoubleLong();
            bits.Double = value;
            crc = UpdateCrc(crc, bits.Long);
        }

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

        #endregion

        /// <summary>
        /// Computes CRC32 checksum across all active entity states at the given simulation tick.
        /// Zero GC allocations.
        /// </summary>
        public static uint Compute(int tick, CYGEntityState[] entities, int entityCount)
        {
            uint crc = 0xFFFFFFFFu;

            // Mix in simulation tick
            crc = UpdateCrc(crc, tick);
            crc = UpdateCrc(crc, entityCount);

            for (int i = 0; i < entityCount; i++)
            {
                ref readonly CYGEntityState state = ref entities[i];
                if (!state.IsActive) continue;

                crc = UpdateCrc(crc, state.EntityId);
                crc = UpdateCrc(crc, state.Position.x.RawValue);
                crc = UpdateCrc(crc, state.Position.y.RawValue);
                crc = UpdateCrc(crc, state.Position.z.RawValue);
                crc = UpdateCrc(crc, state.Velocity.x.RawValue);
                crc = UpdateCrc(crc, state.Velocity.y.RawValue);
                crc = UpdateCrc(crc, state.Velocity.z.RawValue);
                crc = UpdateCrc(crc, state.Rotation.x.RawValue);
                crc = UpdateCrc(crc, state.Rotation.y.RawValue);
                crc = UpdateCrc(crc, state.Rotation.z.RawValue);
                crc = UpdateCrc(crc, state.Rotation.w.RawValue);
                crc = UpdateCrc(crc, state.Health);
            }

            return crc ^ 0xFFFFFFFFu;
        }

        /// <summary>
        /// Computes 32-bit FNV-1a hash across all active entity states at the given tick.
        /// Zero GC allocations.
        /// </summary>
        public static uint ComputeFNV1a(int tick, CYGEntityState[] entities, int entityCount)
        {
            const uint fnvPrime = 16777619u;
            uint hash = 2166136261u;

            void MixByte(byte b)
            {
                hash ^= b;
                hash *= fnvPrime;
            }

            void MixInt(int val)
            {
                MixByte((byte)(val & 0xFF));
                MixByte((byte)((val >> 8) & 0xFF));
                MixByte((byte)((val >> 16) & 0xFF));
                MixByte((byte)((val >> 24) & 0xFF));
            }

            void MixLong(long val)
            {
                MixByte((byte)(val & 0xFF));
                MixByte((byte)((val >> 8) & 0xFF));
                MixByte((byte)((val >> 16) & 0xFF));
                MixByte((byte)((val >> 24) & 0xFF));
                MixByte((byte)((val >> 32) & 0xFF));
                MixByte((byte)((val >> 40) & 0xFF));
                MixByte((byte)((val >> 48) & 0xFF));
                MixByte((byte)((val >> 56) & 0xFF));
            }

            MixInt(tick);
            MixInt(entityCount);

            for (int i = 0; i < entityCount; i++)
            {
                ref readonly CYGEntityState state = ref entities[i];
                if (!state.IsActive) continue;

                MixInt(state.EntityId);
                MixLong(state.Position.x.RawValue);
                MixLong(state.Position.y.RawValue);
                MixLong(state.Position.z.RawValue);
                MixLong(state.Velocity.x.RawValue);
                MixLong(state.Velocity.y.RawValue);
                MixLong(state.Velocity.z.RawValue);
                MixLong(state.Rotation.x.RawValue);
                MixLong(state.Rotation.y.RawValue);
                MixLong(state.Rotation.z.RawValue);
                MixLong(state.Rotation.w.RawValue);
                MixInt(state.Health);
            }

            return hash;
        }

        /// <summary>
        /// Compares two checksums and reports desync status.
        /// </summary>
        public static bool CheckDesync(int tick, uint localChecksum, uint remoteChecksum, out string desyncReport)
        {
            if (localChecksum == remoteChecksum)
            {
                desyncReport = null;
                return false;
            }

            desyncReport = $"[CYG DESYNC] Mismatch at Tick {tick}: Local=0x{localChecksum:X8}, Remote=0x{remoteChecksum:X8}";
            return true;
        }

        /// <summary>
        /// Formats checksum as 8-character hex string.
        /// </summary>
        public static string ToHexString(uint checksum) => $"0x{checksum:X8}";
    }
}
