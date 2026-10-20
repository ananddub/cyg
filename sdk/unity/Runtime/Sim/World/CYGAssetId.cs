using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Stable id for an authored asset. The DSL emits strongly typed wrappers
    /// (for example <c>WeaponRef</c>) that embed this raw id.
    /// </summary>
    [StructLayout(LayoutKind.Sequential, Pack = 1)]
    public struct CYGAssetId : IEquatable<CYGAssetId>, ICYGValue
    {
        /// <summary>Wire size of an asset id in bytes.</summary>
        public const int SizeInBytes = 4;

        /// <summary>Raw asset id. Zero means <see cref="None"/>.</summary>
        public int Id;

        /// <summary>The canonical null asset id.</summary>
        public static readonly CYGAssetId None = new CYGAssetId(0);

        public CYGAssetId(int id)
        {
            Id = id;
        }

        /// <summary>True when this id does not point at any asset.</summary>
        public bool IsNone
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return Id == 0; }
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
            CYGWire.WriteInt32(buffer, offset, Id);
            return SizeInBytes;
        }

        public void ReadFrom(byte[] buffer, int offset)
        {
            CYGWire.RequireReadable(buffer, offset, SizeInBytes);
            Id = CYGWire.ReadInt32(buffer, offset);
        }

        public readonly void ComputeChecksum(ref uint crc)
        {
            CYGChecksum.HashInt(ref crc, Id);
        }

        #endregion

        #region Equality

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public readonly bool Equals(CYGAssetId other)
        {
            return Id == other.Id;
        }

        public readonly override bool Equals(object obj)
        {
            return obj is CYGAssetId other && Equals(other);
        }

        public readonly override int GetHashCode()
        {
            return Id;
        }

        public static bool operator ==(CYGAssetId left, CYGAssetId right)
        {
            return left.Equals(right);
        }

        public static bool operator !=(CYGAssetId left, CYGAssetId right)
        {
            return !left.Equals(right);
        }

        public readonly override string ToString()
        {
            return IsNone ? "AssetId(None)" : $"AssetId({Id})";
        }

        #endregion
    }
}
