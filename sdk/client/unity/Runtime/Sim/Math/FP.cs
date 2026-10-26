//-----------------------------------------------------------------------------
// FP.cs
// High-performance Q32.32 Fixed-Point struct for deterministic game simulation.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Deterministic 32.32 fixed-point numeric value.
    /// Fractional precision: 32 bits (~2.328e-10 resolution).
    /// Range: -2,147,483,648 to +2,147,483,647.
    /// </summary>
    [Serializable]
    public struct FP : IEquatable<FP>, IComparable<FP>, ICYGValue
    {
        public long RawValue;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FP(long rawValue)
        {
            RawValue = rawValue;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FP(int intValue)
        {
            RawValue = Fixed64Math.FromInt(intValue);
        }

        public long Raw
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => RawValue;
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            set => RawValue = value;
        }

        #region Constants

        public static readonly FP Zero = new FP(Fixed64Math.Zero);
        public static readonly FP One = new FP(Fixed64Math.One);
        public static readonly FP Half = new FP(Fixed64Math.Half);
        public static readonly FP Two = new FP(Fixed64Math.Two);
        public static readonly FP Pi = new FP(Fixed64Math.Pi);
        public static readonly FP TwoPi = new FP(Fixed64Math.TwoPi);
        public static readonly FP HalfPi = new FP(Fixed64Math.HalfPi);
        public static readonly FP E = new FP(Fixed64Math.E);
        public static readonly FP MinValue = new FP(Fixed64Math.MinValue);
        public static readonly FP MaxValue = new FP(Fixed64Math.MaxValue);
        public static readonly FP Epsilon = new FP(Fixed64Math.Epsilon);
        public static readonly FP Rad2Deg = new FP(Fixed64Math.Rad2Deg);
        public static readonly FP Deg2Rad = new FP(Fixed64Math.Deg2Rad);

        #endregion

        #region Factory Methods & Conversions

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP FromRaw(long raw) => new FP(raw);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP FromInt(int value) => new FP(Fixed64Math.FromInt(value));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP FromFloat(float value) => new FP(Fixed64Math.FromFloat(value));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP FromDouble(double value) => new FP(Fixed64Math.FromDouble(value));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP FromFraction(long numerator, long denominator)
        {
            return new FP(Fixed64Math.DivPrecise(Fixed64Math.FromInt((int)numerator), Fixed64Math.FromInt((int)denominator)));
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public float AsFloat() => Fixed64Math.ToFloat(RawValue);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public double AsDouble() => Fixed64Math.ToDouble(RawValue);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public int AsInt() => Fixed64Math.ToInt(RawValue);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public long AsLong() => RawValue >> Fixed64Math.Shift;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static implicit operator FP(int value) => FromInt(value);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static explicit operator FP(float value) => FromFloat(value);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static explicit operator FP(double value) => FromDouble(value);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static explicit operator float(FP value) => value.AsFloat();

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static explicit operator double(FP value) => value.AsDouble();

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static explicit operator int(FP value) => value.AsInt();

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static explicit operator long(FP value) => value.AsLong();

        #endregion

        #region Operators

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP operator +(FP a, FP b) => new FP(a.RawValue + b.RawValue);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP operator -(FP a, FP b) => new FP(a.RawValue - b.RawValue);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP operator -(FP a) => new FP(-a.RawValue);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP operator *(FP a, FP b) => new FP(Fixed64Math.Mul(a.RawValue, b.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP operator *(FP a, int b) => new FP(a.RawValue * b);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP operator *(int a, FP b) => new FP(a * b.RawValue);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP operator /(FP a, FP b) => new FP(Fixed64Math.DivPrecise(a.RawValue, b.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP operator /(FP a, int b) => new FP(a.RawValue / b);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP operator %(FP a, FP b) => new FP(Fixed64Math.Mod(a.RawValue, b.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static bool operator ==(FP a, FP b) => a.RawValue == b.RawValue;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static bool operator !=(FP a, FP b) => a.RawValue != b.RawValue;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static bool operator <(FP a, FP b) => a.RawValue < b.RawValue;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static bool operator >(FP a, FP b) => a.RawValue > b.RawValue;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static bool operator <=(FP a, FP b) => a.RawValue <= b.RawValue;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static bool operator >=(FP a, FP b) => a.RawValue >= b.RawValue;

        #endregion

        #region Static Math Methods

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Abs(FP x) => new FP(Fixed64Math.Abs(x.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static int Sign(FP x) => Fixed64Math.Sign(x.RawValue);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Floor(FP x) => new FP(Fixed64Math.Floor(x.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Ceil(FP x) => new FP(Fixed64Math.Ceil(x.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Round(FP x) => new FP(Fixed64Math.Round(x.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Min(FP a, FP b) => new FP(Fixed64Math.Min(a.RawValue, b.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Max(FP a, FP b) => new FP(Fixed64Math.Max(a.RawValue, b.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Clamp(FP val, FP min, FP max) => new FP(Fixed64Math.Clamp(val.RawValue, min.RawValue, max.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Clamp01(FP val) => Clamp(val, Zero, One);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Lerp(FP a, FP b, FP t) => a + (b - a) * Clamp01(t);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP LerpUnclamped(FP a, FP b, FP t) => a + (b - a) * t;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Sqrt(FP x) => new FP(Fixed64Math.SqrtPrecise(x.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP SqrtPrecise(FP x) => new FP(Fixed64Math.SqrtPrecise(x.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP RSqrt(FP x) => new FP(Fixed64Math.RSqrt(x.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Rcp(FP x) => new FP(Fixed64Math.Rcp(x.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Sin(FP x) => new FP(Fixed64Math.Sin(x.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Cos(FP x) => new FP(Fixed64Math.Cos(x.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Tan(FP x) => new FP(Fixed64Math.Tan(x.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Atan2(FP y, FP x) => new FP(Fixed64Math.Atan2(y.RawValue, x.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Asin(FP x) => new FP(Fixed64Math.Asin(x.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Acos(FP x) => new FP(Fixed64Math.Acos(x.RawValue));

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Atan(FP x) => new FP(Fixed64Math.Atan(x.RawValue));

        #endregion

        #region CYG Wire Codec

        public const int SizeInBytes = 8;

        public readonly int WireSize => SizeInBytes;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public readonly int WriteTo(byte[] buffer, int offset)
        {
            CYGWire.RequireWritable(buffer, offset, SizeInBytes);
            CYGWire.WriteInt64(buffer, offset, RawValue);
            return SizeInBytes;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void ReadFrom(byte[] buffer, int offset)
        {
            CYGWire.RequireReadable(buffer, offset, SizeInBytes);
            RawValue = CYGWire.ReadInt64(buffer, offset);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public readonly void ComputeChecksum(ref uint crc)
        {
            CYGChecksum.HashLong(ref crc, RawValue);
        }

        #endregion

        #region Common Interfaces

        public bool Equals(FP other) => RawValue == other.RawValue;
        public override bool Equals(object obj) => obj is FP other && Equals(other);
        public override int GetHashCode() => RawValue.GetHashCode();
        public int CompareTo(FP other) => RawValue.CompareTo(other.RawValue);
        public override string ToString() => Fixed64Math.ToString(RawValue);

        #endregion
    }
}
