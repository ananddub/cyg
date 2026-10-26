using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Deterministic PCG32 random session. The full 128-bit state can be stored
    /// in a component or snapshot, so a rollback restores the exact sequence.
    /// Never use System.Random or UnityEngine.Random inside the simulation.
    /// </summary>
    [Serializable]
    public struct CYGRngState : ICYGValue, IEquatable<CYGRngState>
    {
        public const int SizeInBytes = 16;

        private const ulong Multiplier = 6364136223846793005UL;

        public ulong State;
        public ulong Increment;

        public CYGRngState(ulong seed, ulong sequence = 0)
        {
            State = 0UL;
            Increment = (sequence << 1) | 1UL;
            NextUInt();
            State = unchecked(State + seed);
            NextUInt();
        }

        public readonly int WireSize
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return SizeInBytes; }
        }

        /// <summary>Reseeds in place. The sequence selects an independent stream.</summary>
        public void Seed(ulong seed, ulong sequence = 0)
        {
            this = new CYGRngState(seed, sequence);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public uint NextUInt()
        {
            ulong old = State;
            State = unchecked(old * Multiplier + Increment);
            uint xorshifted = (uint)(((old >> 18) ^ old) >> 27);
            int rotation = (int)(old >> 59);
            return (xorshifted >> rotation) | (xorshifted << ((-rotation) & 31));
        }

        /// <summary>Returns a value in [minInclusive, maxExclusive).</summary>
        public uint NextUInt(uint minInclusive, uint maxExclusive)
        {
            if (maxExclusive <= minInclusive)
            {
                throw new ArgumentOutOfRangeException(nameof(maxExclusive), "maxExclusive must be greater than minInclusive.");
            }

            uint range = maxExclusive - minInclusive;
            uint threshold = unchecked(0u - range) % range;
            uint value;
            do
            {
                value = NextUInt();
            }
            while (value < threshold);

            return minInclusive + (value % range);
        }

        /// <summary>Returns a value in [minInclusive, maxExclusive).</summary>
        public int NextInt(int minInclusive, int maxExclusive)
        {
            if (maxExclusive <= minInclusive)
            {
                throw new ArgumentOutOfRangeException(nameof(maxExclusive), "maxExclusive must be greater than minInclusive.");
            }

            uint range = unchecked((uint)((long)maxExclusive - minInclusive));
            uint threshold = unchecked(0u - range) % range;
            uint value;
            do
            {
                value = NextUInt();
            }
            while (value < threshold);

            return (int)(minInclusive + (long)(value % range));
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool NextBool()
        {
            return (NextUInt() & 1u) != 0;
        }

        /// <summary>Returns a fixed-point value in [0, 1).</summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FP NextFP()
        {
            return FP.FromRaw(NextUInt());
        }

        /// <summary>Returns a fixed-point value in [minInclusive, maxExclusive).</summary>
        public FP NextFP(FP minInclusive, FP maxExclusive)
        {
            if (maxExclusive.RawValue <= minInclusive.RawValue)
            {
                throw new ArgumentOutOfRangeException(nameof(maxExclusive), "maxExclusive must be greater than minInclusive.");
            }

            return minInclusive + (maxExclusive - minInclusive) * NextFP();
        }

        public void Shuffle<T>(T[] values)
        {
            if (values == null)
            {
                throw new ArgumentNullException(nameof(values));
            }

            for (int i = values.Length - 1; i > 0; i--)
            {
                int swap = NextInt(0, i + 1);
                T value = values[i];
                values[i] = values[swap];
                values[swap] = value;
            }
        }

        public readonly int WriteTo(byte[] buffer, int offset)
        {
            CYGWire.RequireWritable(buffer, offset, SizeInBytes);
            CYGWire.WriteUInt64(buffer, offset, State);
            CYGWire.WriteUInt64(buffer, offset + 8, Increment);
            return SizeInBytes;
        }

        public void ReadFrom(byte[] buffer, int offset)
        {
            CYGWire.RequireReadable(buffer, offset, SizeInBytes);
            State = CYGWire.ReadUInt64(buffer, offset);
            Increment = CYGWire.ReadUInt64(buffer, offset + 8);
        }

        public readonly void ComputeChecksum(ref uint crc)
        {
            CYGChecksum.HashULong(ref crc, State);
            CYGChecksum.HashULong(ref crc, Increment);
        }

        public readonly bool Equals(CYGRngState other)
        {
            return State == other.State && Increment == other.Increment;
        }

        public readonly override bool Equals(object obj)
        {
            return obj is CYGRngState other && Equals(other);
        }

        public readonly override int GetHashCode()
        {
            unchecked
            {
                return (State.GetHashCode() * 397) ^ Increment.GetHashCode();
            }
        }

        public static bool operator ==(CYGRngState left, CYGRngState right) => left.Equals(right);

        public static bool operator !=(CYGRngState left, CYGRngState right) => !left.Equals(right);
    }
}
