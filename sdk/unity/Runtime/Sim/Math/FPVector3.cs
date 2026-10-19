//-----------------------------------------------------------------------------
// FPVector3.cs
// 3D Fixed-Point Vector for deterministic simulation.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// 3D fixed-point vector.
    /// </summary>
    [Serializable]
    public struct FPVector3 : IEquatable<FPVector3>
    {
        public FP x;
        public FP y;
        public FP z;

        public FP X
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => x;
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            set => x = value;
        }

        public FP Y
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => y;
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            set => y = value;
        }

        public FP Z
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => z;
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            set => z = value;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPVector3(FP x, FP y, FP z)
        {
            this.x = x;
            this.y = y;
            this.z = z;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPVector3(int x, int y, int z)
        {
            this.x = FP.FromInt(x);
            this.y = FP.FromInt(y);
            this.z = FP.FromInt(z);
        }

        #region Constants

        public static readonly FPVector3 Zero = new FPVector3(FP.Zero, FP.Zero, FP.Zero);
        public static readonly FPVector3 One = new FPVector3(FP.One, FP.One, FP.One);
        public static readonly FPVector3 Forward = new FPVector3(FP.Zero, FP.Zero, FP.One);
        public static readonly FPVector3 Back = new FPVector3(FP.Zero, FP.Zero, -FP.One);
        public static readonly FPVector3 Up = new FPVector3(FP.Zero, FP.One, FP.Zero);
        public static readonly FPVector3 Down = new FPVector3(FP.Zero, -FP.One, FP.Zero);
        public static readonly FPVector3 Right = new FPVector3(FP.One, FP.Zero, FP.Zero);
        public static readonly FPVector3 Left = new FPVector3(-FP.One, FP.Zero, FP.Zero);

        #endregion

        #region Properties & Methods

        public FP SqrMagnitude
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => x * x + y * y + z * z;
        }

        public FP Magnitude
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => FP.Sqrt(SqrMagnitude);
        }

        public FPVector3 Normalized
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get
            {
                FP mag = Magnitude;
                return mag > FP.Zero ? this / mag : Zero;
            }
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void Normalize()
        {
            FP mag = Magnitude;
            if (mag > FP.Zero)
            {
                x /= mag;
                y /= mag;
                z /= mag;
            }
            else
            {
                x = FP.Zero;
                y = FP.Zero;
                z = FP.Zero;
            }
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Dot(FPVector3 a, FPVector3 b) => a.x * b.x + a.y * b.y + a.z * b.z;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 Cross(FPVector3 a, FPVector3 b) => new FPVector3(
            a.y * b.z - a.z * b.y,
            a.z * b.x - a.x * b.z,
            a.x * b.y - a.y * b.x
        );

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Distance(FPVector3 a, FPVector3 b) => (a - b).Magnitude;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP DistanceSquared(FPVector3 a, FPVector3 b) => (a - b).SqrMagnitude;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 Lerp(FPVector3 a, FPVector3 b, FP t) => a + (b - a) * FP.Clamp01(t);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 LerpUnclamped(FPVector3 a, FPVector3 b, FP t) => a + (b - a) * t;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 Min(FPVector3 a, FPVector3 b) => new FPVector3(
            FP.Min(a.x, b.x),
            FP.Min(a.y, b.y),
            FP.Min(a.z, b.z)
        );

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 Max(FPVector3 a, FPVector3 b) => new FPVector3(
            FP.Max(a.x, b.x),
            FP.Max(a.y, b.y),
            FP.Max(a.z, b.z)
        );

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 Clamp(FPVector3 value, FPVector3 min, FPVector3 max) => new FPVector3(
            FP.Clamp(value.x, min.x, max.x),
            FP.Clamp(value.y, min.y, max.y),
            FP.Clamp(value.z, min.z, max.z)
        );

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 Project(FPVector3 vector, FPVector3 onNormal)
        {
            FP sqrMag = Dot(onNormal, onNormal);
            if (sqrMag < FP.Epsilon)
                return Zero;
            FP dot = Dot(vector, onNormal);
            return onNormal * (dot / sqrMag);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 ProjectOnPlane(FPVector3 vector, FPVector3 planeNormal)
        {
            return vector - Project(vector, planeNormal);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 Reflect(FPVector3 inDirection, FPVector3 inNormal)
        {
            return inDirection - FP.FromInt(2) * Dot(inDirection, inNormal) * inNormal;
        }

        public static FP Angle(FPVector3 from, FPVector3 to)
        {
            FP denom = FP.Sqrt(from.SqrMagnitude * to.SqrMagnitude);
            if (denom < FP.Epsilon)
                return FP.Zero;
            FP dot = FP.Clamp(Dot(from, to) / denom, -FP.One, FP.One);
            return FP.Acos(dot) * FP.Rad2Deg;
        }

        public static FP SignedAngle(FPVector3 from, FPVector3 to, FPVector3 axis)
        {
            FP unsignedAngle = Angle(from, to);
            FPVector3 cross = Cross(from, to);
            FP sign = FP.Sign(Dot(axis, cross));
            return unsignedAngle * (sign >= 0 ? FP.One : -FP.One);
        }

        public static FPVector3 MoveTowards(FPVector3 current, FPVector3 target, FP maxDistanceDelta)
        {
            FPVector3 delta = target - current;
            FP sqrDist = delta.SqrMagnitude;
            if (sqrDist == FP.Zero || (maxDistanceDelta >= FP.Zero && sqrDist <= maxDistanceDelta * maxDistanceDelta))
                return target;
            FP dist = FP.Sqrt(sqrDist);
            return current + delta / dist * maxDistanceDelta;
        }

        public static FPVector3 ClampMagnitude(FPVector3 vector, FP maxLength)
        {
            FP sqrMag = vector.SqrMagnitude;
            if (sqrMag > maxLength * maxLength)
            {
                FP mag = FP.Sqrt(sqrMag);
                return (vector / mag) * maxLength;
            }
            return vector;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPVector2 ToFPVector2XZ() => new FPVector2(x, z);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPVector2 ToFPVector2XY() => new FPVector2(x, y);

        #endregion

        #region Operators

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 operator +(FPVector3 a, FPVector3 b) => new FPVector3(a.x + b.x, a.y + b.y, a.z + b.z);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 operator -(FPVector3 a, FPVector3 b) => new FPVector3(a.x - b.x, a.y - b.y, a.z - b.z);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 operator -(FPVector3 a) => new FPVector3(-a.x, -a.y, -a.z);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 operator *(FPVector3 a, FP d) => new FPVector3(a.x * d, a.y * d, a.z * d);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 operator *(FP d, FPVector3 a) => new FPVector3(a.x * d, a.y * d, a.z * d);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 operator *(FPVector3 a, FPVector3 b) => new FPVector3(a.x * b.x, a.y * b.y, a.z * b.z);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 operator /(FPVector3 a, FP d) => new FPVector3(a.x / d, a.y / d, a.z / d);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static bool operator ==(FPVector3 a, FPVector3 b) => a.x == b.x && a.y == b.y && a.z == b.z;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static bool operator !=(FPVector3 a, FPVector3 b) => !(a == b);

        #endregion

        #region Interfaces

        public bool Equals(FPVector3 other) => x == other.x && y == other.y && z == other.z;
        public override bool Equals(object obj) => obj is FPVector3 other && Equals(other);
        public override int GetHashCode() => x.GetHashCode() ^ (y.GetHashCode() * 397) ^ (z.GetHashCode() * 7919);
        public override string ToString() => $"({x}, {y}, {z})";

        #endregion
    }
}
