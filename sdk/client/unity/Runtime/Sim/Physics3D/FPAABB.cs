//-----------------------------------------------------------------------------
// FPAABB.cs
// 3D Axis-Aligned Bounding Box for deterministic physics queries.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Axis-Aligned Bounding Box with fixed-point coordinates.
    /// </summary>
    [Serializable]
    public struct FPAABB : IEquatable<FPAABB>, ICYGValue
    {
        public FPVector3 Min;
        public FPVector3 Max;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPAABB(FPVector3 min, FPVector3 max)
        {
            Min = min;
            Max = max;
        }

        public FPVector3 Center
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => (Min + Max) * FP.Half;
        }

        public FPVector3 Extents
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => (Max - Min) * FP.Half;
        }

        public FPVector3 Size
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => Max - Min;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPAABB FromCenterSize(FPVector3 center, FPVector3 size)
        {
            FPVector3 half = size * FP.Half;
            return new FPAABB(center - half, center + half);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPAABB FromCenterExtents(FPVector3 center, FPVector3 extents)
        {
            return new FPAABB(center - extents, center + extents);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool Contains(FPVector3 point)
        {
            return point.x >= Min.x && point.x <= Max.x
                && point.y >= Min.y && point.y <= Max.y
                && point.z >= Min.z && point.z <= Max.z;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool Contains(FPAABB other)
        {
            return Min.x <= other.Min.x && Max.x >= other.Max.x
                && Min.y <= other.Min.y && Max.y >= other.Max.y
                && Min.z <= other.Min.z && Max.z >= other.Max.z;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool Intersects(FPAABB other)
        {
            return Max.x >= other.Min.x && Min.x <= other.Max.x
                && Max.y >= other.Min.y && Min.y <= other.Max.y
                && Max.z >= other.Min.z && Min.z <= other.Max.z;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool Intersects(FPSphere sphere)
        {
            return DistanceSquared(sphere.Center) <= sphere.Radius * sphere.Radius;
        }

        public bool Intersects(FPCapsule capsule)
        {
            // Test closest point on capsule segment to AABB
            FPVector3 closest = ClosestPoint(capsule.ClosestPointOnSegment(Center));
            return capsule.DistanceSquaredToSegment(closest) <= capsule.Radius * capsule.Radius;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPVector3 ClosestPoint(FPVector3 point)
        {
            return new FPVector3(
                FP.Clamp(point.x, Min.x, Max.x),
                FP.Clamp(point.y, Min.y, Max.y),
                FP.Clamp(point.z, Min.z, Max.z)
            );
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FP DistanceSquared(FPVector3 point)
        {
            FP sqDist = FP.Zero;

            if (point.x < Min.x) { FP d = Min.x - point.x; sqDist += d * d; }
            else if (point.x > Max.x) { FP d = point.x - Max.x; sqDist += d * d; }

            if (point.y < Min.y) { FP d = Min.y - point.y; sqDist += d * d; }
            else if (point.y > Max.y) { FP d = point.y - Max.y; sqDist += d * d; }

            if (point.z < Min.z) { FP d = Min.z - point.z; sqDist += d * d; }
            else if (point.z > Max.z) { FP d = point.z - Max.z; sqDist += d * d; }

            return sqDist;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FP Distance(FPVector3 point) => FP.Sqrt(DistanceSquared(point));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void Encapsulate(FPVector3 point)
        {
            Min = FPVector3.Min(Min, point);
            Max = FPVector3.Max(Max, point);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void Encapsulate(FPAABB other)
        {
            Min = FPVector3.Min(Min, other.Min);
            Max = FPVector3.Max(Max, other.Max);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void Expand(FP amount)
        {
            FPVector3 v = new FPVector3(amount, amount, amount);
            Min -= v;
            Max += v;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void Expand(FPVector3 amount)
        {
            Min -= amount;
            Max += amount;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void Translate(FPVector3 offset)
        {
            Min += offset;
            Max += offset;
        }

        #region CYG Wire Codec

        public const int SizeInBytes = 48;

        public readonly int WireSize => SizeInBytes;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public readonly int WriteTo(byte[] buffer, int offset)
        {
            CYGWire.RequireWritable(buffer, offset, SizeInBytes);
            Min.WriteTo(buffer, offset);
            Max.WriteTo(buffer, offset + FPVector3.SizeInBytes);
            return SizeInBytes;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void ReadFrom(byte[] buffer, int offset)
        {
            CYGWire.RequireReadable(buffer, offset, SizeInBytes);
            Min.ReadFrom(buffer, offset);
            Max.ReadFrom(buffer, offset + FPVector3.SizeInBytes);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public readonly void ComputeChecksum(ref uint crc)
        {
            Min.ComputeChecksum(ref crc);
            Max.ComputeChecksum(ref crc);
        }

        #endregion

        #region Interfaces

        public bool Equals(FPAABB other) => Min == other.Min && Max == other.Max;
        public override bool Equals(object obj) => obj is FPAABB other && Equals(other);
        public override int GetHashCode() => Min.GetHashCode() ^ (Max.GetHashCode() * 397);
        public override string ToString() => $"AABB(Min:{Min}, Max:{Max})";

        #endregion
    }
}
