//-----------------------------------------------------------------------------
// FPBounds.cs
// Axis-aligned bounds expressed by center and extents.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Axis-aligned bounds described by a center and half-size extents.
    /// Use this when authoring volumes; use <see cref="FPAABB"/> when the
    /// min/max corners are the natural representation.
    /// </summary>
    [Serializable]
    public struct FPBounds : IEquatable<FPBounds>, ICYGValue
    {
        public FPVector3 Center;
        public FPVector3 Extents;

        public FPBounds(FPVector3 center, FPVector3 extents)
        {
            Center = center;
            Extents = extents;
        }

        public FPVector3 Size
        {
            get { return Extents + Extents; }
        }

        public FPVector3 Min
        {
            get { return Center - Extents; }
        }

        public FPVector3 Max
        {
            get { return Center + Extents; }
        }

        public bool Contains(FPVector3 point)
        {
            FPVector3 min = Min;
            FPVector3 max = Max;
            return point.x >= min.x && point.x <= max.x
                && point.y >= min.y && point.y <= max.y
                && point.z >= min.z && point.z <= max.z;
        }

        public FPAABB ToAABB()
        {
            FPAABB aabb = default(FPAABB);
            aabb.Min = Min;
            aabb.Max = Max;
            return aabb;
        }

        public static FPBounds FromAABB(FPAABB aabb)
        {
            FPBounds bounds = default(FPBounds);
            bounds.Center = (aabb.Min + aabb.Max) * FP.Half;
            bounds.Extents = (aabb.Max - aabb.Min) * FP.Half;
            return bounds;
        }

        public static bool operator ==(FPBounds a, FPBounds b) => a.Equals(b);

        public static bool operator !=(FPBounds a, FPBounds b) => !a.Equals(b);

        #region CYG Wire Codec

        public const int SizeInBytes = 48;

        public readonly int WireSize => SizeInBytes;

        public readonly int WriteTo(byte[] buffer, int offset)
        {
            CYGWire.RequireWritable(buffer, offset, SizeInBytes);
            Center.WriteTo(buffer, offset);
            Extents.WriteTo(buffer, offset + FPVector3.SizeInBytes);
            return SizeInBytes;
        }

        public void ReadFrom(byte[] buffer, int offset)
        {
            CYGWire.RequireReadable(buffer, offset, SizeInBytes);
            Center.ReadFrom(buffer, offset);
            Extents.ReadFrom(buffer, offset + FPVector3.SizeInBytes);
        }

        public readonly void ComputeChecksum(ref uint crc)
        {
            Center.ComputeChecksum(ref crc);
            Extents.ComputeChecksum(ref crc);
        }

        #endregion

        #region Interfaces

        public readonly bool Equals(FPBounds other)
        {
            return Center == other.Center && Extents == other.Extents;
        }

        public readonly override bool Equals(object obj) => obj is FPBounds other && Equals(other);

        public readonly override int GetHashCode() => (Center.GetHashCode() * 397) ^ Extents.GetHashCode();

        public readonly override string ToString() => "Center: " + Center + ", Extents: " + Extents;

        #endregion
    }
}
