using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Stable reference to an entity slot. The pair of index and version makes a
    /// reference self-validating: once a slot is destroyed and recycled the old
    /// reference no longer matches the slot's version.
    /// </summary>
    [StructLayout(LayoutKind.Sequential, Pack = 1)]
    public struct CYGEntityRef : IEquatable<CYGEntityRef>, ICYGValue
    {
        /// <summary>Wire size of an entity reference in bytes.</summary>
        public const int SizeInBytes = 8;

        /// <summary>Zero-based slot index. Negative only for <see cref="None"/>.</summary>
        public int Index;

        /// <summary>Slot generation. Zero means "no entity".</summary>
        public int Version;

        /// <summary>The canonical null reference. Version 0 is never assigned by a world.</summary>
        public static readonly CYGEntityRef None = new CYGEntityRef(-1, 0);

        public CYGEntityRef(int index, int version)
        {
            Index = index;
            Version = version;
        }

        /// <summary>True when this reference does not point at any entity.</summary>
        public bool IsNone
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return Version == 0; }
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
            CYGWire.WriteInt32(buffer, offset, Index);
            CYGWire.WriteInt32(buffer, offset + 4, Version);
            return SizeInBytes;
        }

        public void ReadFrom(byte[] buffer, int offset)
        {
            CYGWire.RequireReadable(buffer, offset, SizeInBytes);
            Index = CYGWire.ReadInt32(buffer, offset);
            Version = CYGWire.ReadInt32(buffer, offset + 4);
        }

        public readonly void ComputeChecksum(ref uint crc)
        {
            CYGChecksum.HashInt(ref crc, Index);
            CYGChecksum.HashInt(ref crc, Version);
        }

        #endregion

        #region Equality

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public readonly bool Equals(CYGEntityRef other)
        {
            return Index == other.Index && Version == other.Version;
        }

        public readonly override bool Equals(object obj)
        {
            return obj is CYGEntityRef other && Equals(other);
        }

        public readonly override int GetHashCode()
        {
            unchecked
            {
                return (Index * 397) ^ Version;
            }
        }

        public static bool operator ==(CYGEntityRef left, CYGEntityRef right)
        {
            return left.Equals(right);
        }

        public static bool operator !=(CYGEntityRef left, CYGEntityRef right)
        {
            return !left.Equals(right);
        }

        public readonly override string ToString()
        {
            return IsNone ? "EntityRef(None)" : $"EntityRef({Index}, v{Version})";
        }

        #endregion
    }
}
