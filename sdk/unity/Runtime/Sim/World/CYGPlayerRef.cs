using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Compact reference to a player. Value 0 means "no player"; player slots
    /// are assigned by the simulation layer, not by a world.
    /// </summary>
    [StructLayout(LayoutKind.Sequential, Pack = 1)]
    public struct CYGPlayerRef : IEquatable<CYGPlayerRef>, ICYGValue
    {
        /// <summary>Wire size of a player reference in bytes.</summary>
        public const int SizeInBytes = 2;

        /// <summary>Player id. Zero means <see cref="None"/>.</summary>
        public ushort Value;

        /// <summary>The canonical null player reference.</summary>
        public static readonly CYGPlayerRef None = new CYGPlayerRef(0);

        public CYGPlayerRef(ushort value)
        {
            Value = value;
        }

        /// <summary>True when this reference does not point at any player.</summary>
        public bool IsNone
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return Value == 0; }
        }

        #region CYG Wire Codec

        public int WireSize
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return SizeInBytes; }
        }

        public readonly int WriteTo(byte[] buffer, int offset)
        {
            CYGWire.RequireWritable(buffer, offset, SizeInBytes);
            CYGWire.WriteUInt16(buffer, offset, Value);
            return SizeInBytes;
        }

        public void ReadFrom(byte[] buffer, int offset)
        {
            CYGWire.RequireReadable(buffer, offset, SizeInBytes);
            Value = CYGWire.ReadUInt16(buffer, offset);
        }

        public readonly void ComputeChecksum(ref uint crc)
        {
            CYGChecksum.HashUInt16(ref crc, Value);
        }

        #endregion

        #region Equality

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public readonly bool Equals(CYGPlayerRef other)
        {
            return Value == other.Value;
        }

        public readonly override bool Equals(object obj)
        {
            return obj is CYGPlayerRef other && Equals(other);
        }

        public readonly override int GetHashCode()
        {
            return Value;
        }

        public static bool operator ==(CYGPlayerRef left, CYGPlayerRef right)
        {
            return left.Equals(right);
        }

        public static bool operator !=(CYGPlayerRef left, CYGPlayerRef right)
        {
            return !left.Equals(right);
        }

        public readonly override string ToString()
        {
            return IsNone ? "PlayerRef(None)" : $"PlayerRef({Value})";
        }

        #endregion
    }
}
