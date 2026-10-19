//-----------------------------------------------------------------------------
// FPCapsule.cs
// 3D Fixed-Point Capsule collision primitive.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// 3D capsule collider definition with fixed-point coordinates.
    /// Represents all points within Radius from the central line segment [PointA, PointB].
    /// </summary>
    [Serializable]
    public struct FPCapsule : IEquatable<FPCapsule>
    {
        public FPVector3 PointA;
        public FPVector3 PointB;
        public FP Radius;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPCapsule(FPVector3 pointA, FPVector3 pointB, FP radius)
        {
            PointA = pointA;
            PointB = pointB;
            Radius = radius;
        }

        /// <summary>
        /// Creates a vertical capsule aligned along the Y-axis.
        /// Center is the geometric center of the capsule. Total height includes both hemispherical caps.
        /// </summary>
        public static FPCapsule CreateVertical(FPVector3 center, FP height, FP radius)
        {
            FP halfHeight = height * FP.Half;
            FP segmentHalf = FP.Max(FP.Zero, halfHeight - radius);
            FPVector3 pointA = center - FPVector3.Up * segmentHalf;
            FPVector3 pointB = center + FPVector3.Up * segmentHalf;
            return new FPCapsule(pointA, pointB, radius);
        }

        public FPVector3 Center
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => (PointA + PointB) * FP.Half;
        }

        public FP Height
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => (PointB - PointA).Magnitude + Radius * FP.FromInt(2);
        }

        public FPVector3 Direction
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => (PointB - PointA).Normalized;
        }

        public FP SegmentLength
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => (PointB - PointA).Magnitude;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPAABB GetAABB()
        {
            FPVector3 r = new FPVector3(Radius, Radius, Radius);
            FPVector3 min = FPVector3.Min(PointA, PointB) - r;
            FPVector3 max = FPVector3.Max(PointA, PointB) + r;
            return new FPAABB(min, max);
        }

        /// <summary>
        /// Finds the closest point on the central segment [PointA, PointB] to the specified point.
        /// </summary>
        public FPVector3 ClosestPointOnSegment(FPVector3 point)
        {
            FPVector3 ab = PointB - PointA;
            FP abLenSq = ab.SqrMagnitude;
            if (abLenSq < FP.Epsilon)
                return PointA;

            FP t = FPVector3.Dot(point - PointA, ab) / abLenSq;
            t = FP.Clamp01(t);
            return PointA + ab * t;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FP DistanceSquaredToSegment(FPVector3 point)
        {
            FPVector3 p = ClosestPointOnSegment(point);
            return FPVector3.DistanceSquared(p, point);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FP DistanceToSegment(FPVector3 point) => FP.Sqrt(DistanceSquaredToSegment(point));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool Contains(FPVector3 point)
        {
            return DistanceSquaredToSegment(point) <= Radius * Radius;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPVector3 ClosestPoint(FPVector3 point)
        {
            FPVector3 segPoint = ClosestPointOnSegment(point);
            FPVector3 diff = point - segPoint;
            FP dist = diff.Magnitude;
            if (dist <= Radius)
                return point;
            return segPoint + (diff / dist) * Radius;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool Intersects(FPSphere sphere)
        {
            FP totalRadius = Radius + sphere.Radius;
            return DistanceSquaredToSegment(sphere.Center) <= totalRadius * totalRadius;
        }

        public bool Intersects(FPAABB aabb)
        {
            return aabb.Intersects(this);
        }

        /// <summary>
        /// Tests intersection between two capsules by calculating closest points between their central segments.
        /// </summary>
        public bool Intersects(FPCapsule other)
        {
            ClosestPointsBetweenSegments(PointA, PointB, other.PointA, other.PointB, out FPVector3 c1, out FPVector3 c2);
            FP totalRadius = Radius + other.Radius;
            return FPVector3.DistanceSquared(c1, c2) <= totalRadius * totalRadius;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void Translate(FPVector3 offset)
        {
            PointA += offset;
            PointB += offset;
        }

        /// <summary>
        /// Robust closest points calculation between two 3D line segments (from Christer Ericson's Real-Time Collision Detection).
        /// </summary>
        public static void ClosestPointsBetweenSegments(
            FPVector3 p1, FPVector3 q1,
            FPVector3 p2, FPVector3 q2,
            out FPVector3 c1, out FPVector3 c2)
        {
            FPVector3 d1 = q1 - p1;
            FPVector3 d2 = q2 - p2;
            FPVector3 r = p1 - p2;

            FP a = FPVector3.Dot(d1, d1);
            FP e = FPVector3.Dot(d2, d2);
            FP f = FPVector3.Dot(d2, r);

            FP s, t;

            if (a <= FP.Epsilon && e <= FP.Epsilon)
            {
                s = FP.Zero;
                t = FP.Zero;
                c1 = p1;
                c2 = p2;
                return;
            }

            if (a <= FP.Epsilon)
            {
                s = FP.Zero;
                t = FP.Clamp01(f / e);
            }
            else
            {
                FP c = FPVector3.Dot(d1, r);
                if (e <= FP.Epsilon)
                {
                    t = FP.Zero;
                    s = FP.Clamp01(-c / a);
                }
                else
                {
                    FP b = FPVector3.Dot(d1, d2);
                    FP denom = a * e - b * b;

                    if (denom > FP.Epsilon)
                        s = FP.Clamp01((b * f - c * e) / denom);
                    else
                        s = FP.Zero;

                    t = (b * s + f) / e;

                    if (t < FP.Zero)
                    {
                        t = FP.Zero;
                        s = FP.Clamp01(-c / a);
                    }
                    else if (t > FP.One)
                    {
                        t = FP.One;
                        s = FP.Clamp01((b - c) / a);
                    }
                }
            }

            c1 = p1 + d1 * s;
            c2 = p2 + d2 * t;
        }

        #region Interfaces

        public bool Equals(FPCapsule other) => PointA == other.PointA && PointB == other.PointB && Radius == other.Radius;
        public override bool Equals(object obj) => obj is FPCapsule other && Equals(other);
        public override int GetHashCode() => PointA.GetHashCode() ^ (PointB.GetHashCode() * 397) ^ (Radius.GetHashCode() * 7919);
        public override string ToString() => $"Capsule(A:{PointA}, B:{PointB}, Radius:{Radius})";

        #endregion
    }
}
