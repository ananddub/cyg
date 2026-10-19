//-----------------------------------------------------------------------------
// FPSphere.cs
// 3D Fixed-Point Sphere collision primitive.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// 3D sphere collider definition with fixed-point coordinates.
    /// </summary>
    [Serializable]
    public struct FPSphere : IEquatable<FPSphere>
    {
        public FPVector3 Center;
        public FP Radius;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPSphere(FPVector3 center, FP radius)
        {
            Center = center;
            Radius = radius;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPAABB GetAABB()
        {
            FPVector3 r = new FPVector3(Radius, Radius, Radius);
            return new FPAABB(Center - r, Center + r);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool Contains(FPVector3 point)
        {
            return FPVector3.DistanceSquared(Center, point) <= Radius * Radius;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool Intersects(FPSphere other)
        {
            FP totalRadius = Radius + other.Radius;
            return FPVector3.DistanceSquared(Center, other.Center) <= totalRadius * totalRadius;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool Intersects(FPAABB aabb)
        {
            return aabb.Intersects(this);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool Intersects(FPCapsule capsule)
        {
            return capsule.Intersects(this);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPVector3 ClosestPoint(FPVector3 point)
        {
            FPVector3 diff = point - Center;
            FP dist = diff.Magnitude;
            if (dist <= Radius)
                return point;
            return Center + (diff / dist) * Radius;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FP Distance(FPVector3 point)
        {
            FP dist = FPVector3.Distance(Center, point);
            return FP.Max(FP.Zero, dist - Radius);
        }

        #region Interfaces

        public bool Equals(FPSphere other) => Center == other.Center && Radius == other.Radius;
        public override bool Equals(object obj) => obj is FPSphere other && Equals(other);
        public override int GetHashCode() => Center.GetHashCode() ^ (Radius.GetHashCode() * 397);
        public override string ToString() => $"Sphere(Center:{Center}, Radius:{Radius})";

        #endregion
    }
}
