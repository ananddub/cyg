//-----------------------------------------------------------------------------
// FPVector2.cs
// 2D Fixed-Point Vector for deterministic simulation.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// 2D fixed-point vector.
    /// </summary>
    [Serializable]
    public struct FPVector2 : IEquatable<FPVector2>
    {
        public FP x;
        public FP y;

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

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPVector2(FP x, FP y)
        {
            this.x = x;
            this.y = y;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPVector2(int x, int y)
        {
            this.x = FP.FromInt(x);
            this.y = FP.FromInt(y);
        }

        #region Constants

        public static readonly FPVector2 Zero = new FPVector2(FP.Zero, FP.Zero);
        public static readonly FPVector2 One = new FPVector2(FP.One, FP.One);
        public static readonly FPVector2 Up = new FPVector2(FP.Zero, FP.One);
        public static readonly FPVector2 Down = new FPVector2(FP.Zero, -FP.One);
        public static readonly FPVector2 Left = new FPVector2(-FP.One, FP.Zero);
        public static readonly FPVector2 Right = new FPVector2(FP.One, FP.Zero);

        #endregion

        #region Properties & Methods

        public FP SqrMagnitude
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => x * x + y * y;
        }

        public FP Magnitude
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => FP.Sqrt(SqrMagnitude);
        }

        public FPVector2 Normalized
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
            }
            else
            {
                x = FP.Zero;
                y = FP.Zero;
            }
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Dot(FPVector2 a, FPVector2 b) => a.x * b.x + a.y * b.y;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Distance(FPVector2 a, FPVector2 b) => (a - b).Magnitude;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP DistanceSquared(FPVector2 a, FPVector2 b) => (a - b).SqrMagnitude;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector2 Lerp(FPVector2 a, FPVector2 b, FP t) => a + (b - a) * FP.Clamp01(t);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector2 LerpUnclamped(FPVector2 a, FPVector2 b, FP t) => a + (b - a) * t;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector2 Min(FPVector2 a, FPVector2 b) => new FPVector2(FP.Min(a.x, b.x), FP.Min(a.y, b.y));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector2 Max(FPVector2 a, FPVector2 b) => new FPVector2(FP.Max(a.x, b.x), FP.Max(a.y, b.y));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector2 Clamp(FPVector2 value, FPVector2 min, FPVector2 max) =>
            new FPVector2(FP.Clamp(value.x, min.x, max.x), FP.Clamp(value.y, min.y, max.y));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector2 Reflect(FPVector2 inDirection, FPVector2 inNormal)
        {
            return inDirection - FP.FromInt(2) * Dot(inDirection, inNormal) * inNormal;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector2 Perpendicular(FPVector2 inDirection) => new FPVector2(-inDirection.y, inDirection.x);

        #endregion

        #region Operators

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector2 operator +(FPVector2 a, FPVector2 b) => new FPVector2(a.x + b.x, a.y + b.y);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector2 operator -(FPVector2 a, FPVector2 b) => new FPVector2(a.x - b.x, a.y - b.y);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector2 operator -(FPVector2 a) => new FPVector2(-a.x, -a.y);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector2 operator *(FPVector2 a, FP d) => new FPVector2(a.x * d, a.y * d);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector2 operator *(FP d, FPVector2 a) => new FPVector2(a.x * d, a.y * d);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector2 operator *(FPVector2 a, FPVector2 b) => new FPVector2(a.x * b.x, a.y * b.y);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector2 operator /(FPVector2 a, FP d) => new FPVector2(a.x / d, a.y / d);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static bool operator ==(FPVector2 a, FPVector2 b) => a.x == b.x && a.y == b.y;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static bool operator !=(FPVector2 a, FPVector2 b) => !(a == b);

        #endregion

        #region Interfaces

        public bool Equals(FPVector2 other) => x == other.x && y == other.y;
        public override bool Equals(object obj) => obj is FPVector2 other && Equals(other);
        public override int GetHashCode() => x.GetHashCode() ^ (y.GetHashCode() * 397);
        public override string ToString() => $"({x}, {y})";

        #endregion
    }
}
