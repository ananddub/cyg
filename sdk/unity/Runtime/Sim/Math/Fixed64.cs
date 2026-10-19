//-----------------------------------------------------------------------------
// Fixed64.cs
// Pure C# Q32.32 fixed-point numeric core and math library.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
// Adapted from battle-tested FixPointCS & FixedMathSharp algorithms.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Core static Q32.32 fixed-point math operations and LUTs.
    /// 32 bits integer, 32 bits fractional precision.
    /// One is represented as 1L &lt;&lt; 32 (4294967296L).
    /// </summary>
    public static class Fixed64Math
    {
        public const int Shift = 32;
        public const long FractionMask = 0xFFFFFFFFL;
        public const ulong UFractionMask = 0xFFFFFFFFUL;

        public const long Zero = 0L;
        public const long One = 1L << Shift; // 4294967296L
        public const long Half = 1L << (Shift - 1); // 2147483648L
        public const long Two = 2L << Shift; // 8589934592L
        public const long Pi = 13493037704L; // 3.14159265358979323846
        public const long TwoPi = 26986075409L;
        public const long HalfPi = 6746518852L;
        public const long E = 11674931555L; // 2.718281828459045
        public const long MinValue = long.MinValue;
        public const long MaxValue = long.MaxValue;
        public const long Epsilon = 1L;
        public const long Rad2Deg = 246083499198L; // 180 / Pi
        public const long Deg2Rad = 74961320L;     // Pi / 180

        private const int RcpHalfPi = 683565276; // (2.0 / Pi) in s2.30

        #region Basic Arithmetic

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long Mul(long a, long b)
        {
            long ai = a >> Shift;
            ulong af = (ulong)(a & FractionMask);
            long bi = b >> Shift;
            ulong bf = (ulong)(b & FractionMask);
            return (long)((af * bf) >> Shift) + ai * b + (long)af * bi;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static int Nlz(ulong x)
        {
            if (x == 0) return 64;
            int n = 0;
            if (x <= 0x00000000FFFFFFFFUL) { n += 32; x <<= 32; }
            if (x <= 0x0000FFFFFFFFFFFFUL) { n += 16; x <<= 16; }
            if (x <= 0x00FFFFFFFFFFFFFFUL) { n += 8; x <<= 8; }
            if (x <= 0x0FFFFFFFFFFFFFFFUL) { n += 4; x <<= 4; }
            if (x <= 0x3FFFFFFFFFFFFFFFUL) { n += 2; x <<= 2; }
            if (x <= 0x7FFFFFFFFFFFFFFFUL) { n += 1; }
            return n;
        }

        /// <summary>
        /// Exact 96-bit by 64-bit integer division without floating point.
        /// </summary>
        public static long DivPrecise(long arg_a, long arg_b)
        {
            if (arg_b == 0)
                throw new DivideByZeroException("Fixed-point division by zero.");

            long sign_dif = arg_a ^ arg_b;

            const ulong b = 0x100000000UL; // Base 2^32
            ulong abs_arg_a = (ulong)(arg_a < 0 ? -arg_a : arg_a);
            ulong u1 = abs_arg_a >> 32;
            ulong u0 = abs_arg_a << 32;
            ulong v = (ulong)(arg_b < 0 ? -arg_b : arg_b);

            if (u1 >= v)
            {
                return sign_dif < 0 ? MinValue : MaxValue;
            }

            int s = Nlz(v);
            v <<= s;
            ulong vn1 = v >> 32;
            ulong vn0 = v & UFractionMask;

            ulong un32 = (u1 << s) | ((s == 0) ? 0UL : (u0 >> (64 - s)));
            ulong un10 = u0 << s;

            ulong un1 = un10 >> 32;
            ulong un0 = un10 & UFractionMask;

            ulong q1 = un32 / vn1;
            ulong rhat = un32 - q1 * vn1;
            while (q1 >= b || (q1 * vn0 > b * rhat + un1))
            {
                q1--;
                rhat += vn1;
                if (rhat >= b) break;
            }

            ulong un21 = un32 * b + un1 - q1 * v;

            ulong q0 = un21 / vn1;
            rhat = un21 - q0 * vn1;
            while (q0 >= b || (q0 * vn0 > b * rhat + un0))
            {
                q0--;
                rhat += vn1;
                if (rhat >= b) break;
            }

            ulong ret = q1 * b + q0;
            return (sign_dif < 0) ? -(long)ret : (long)ret;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long Div(long a, long b)
        {
            return DivPrecise(a, b);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long Mod(long a, long b)
        {
            if (b == 0) throw new DivideByZeroException("Fixed-point modulo by zero.");
            return a % b;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long Abs(long a)
        {
            return (a < 0) ? -a : a;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long Floor(long a)
        {
            return (long)((ulong)a & ~UFractionMask);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long Ceil(long a)
        {
            return (long)(((ulong)a + UFractionMask) & ~UFractionMask);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long Round(long a)
        {
            return Floor(a + Half);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long Fract(long a)
        {
            return a & (long)FractionMask;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long Min(long a, long b) => (a < b) ? a : b;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long Max(long a, long b) => (a > b) ? a : b;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long Clamp(long a, long min, long max)
        {
            if (a < min) return min;
            if (a > max) return max;
            return a;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static int Sign(long a)
        {
            if (a > 0) return 1;
            if (a < 0) return -1;
            return 0;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long Lerp(long a, long b, long t)
        {
            long tb = t;
            long ta = One - tb;
            return Mul(a, ta) + Mul(b, tb);
        }

        #endregion

        #region Sqrt & Reciprocals

        /// <summary>
        /// Exact integer digit-by-digit square root.
        /// </summary>
        public static long SqrtPrecise(long a)
        {
            if (a <= 0) return 0L;

            ulong r = (ulong)a;
            ulong b = 0x4000000000000000UL;
            ulong q = 0UL;
            while (b > 0x40UL)
            {
                ulong t = q + b;
                if (r >= t)
                {
                    r -= t;
                    q = t + b;
                }
                r <<= 1;
                b >>= 1;
            }
            q >>= 16;
            return (long)q;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long Sqrt(long a)
        {
            return SqrtPrecise(a);
        }

        public static long RSqrt(long x)
        {
            if (x <= 0) return 0L;
            return DivPrecise(One, SqrtPrecise(x));
        }

        public static long Rcp(long x)
        {
            if (x == 0) throw new DivideByZeroException("Reciprocal of zero.");
            return DivPrecise(One, x);
        }

        #endregion

        #region Trigonometry (LUT & Polynomial)

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static int Qmul30(int a, int b)
        {
            return (int)((long)a * (long)b >> 30);
        }

        // Precision: 27.13 bits
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private static int SinPoly4(int a)
        {
            int y = Qmul30(a, 162679);
            y = Qmul30(a, y + -5018587);
            y = Qmul30(a, y + 85566362);
            y = Qmul30(a, y + -693598342);
            y = y + 1686629713; // 1.57079632679... in s2.30
            return y;
        }

        private static int UnitSin(int z)
        {
            // Mirror [1, 3] range to [-1, 1]
            if ((z ^ (z << 1)) < 0)
                z = (1 << 31) - z;

            int zz = Qmul30(z, z);
            return Qmul30(SinPoly4(zz), z);
        }

        private static int MulIntLongLow(int a, long b)
        {
            int bi = (int)(b >> Shift);
            long bf = b & FractionMask;
            return (int)((ulong)(a * bf) >> Shift) + a * bi;
        }

        public static long Sin(long x)
        {
            // Map [0, 2pi] to [0, 4] in s2.30
            int z = MulIntLongLow(RcpHalfPi, x);
            return (long)UnitSin(z) << 2;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long Cos(long x)
        {
            return Sin(x + HalfPi);
        }

        public static long Tan(long x)
        {
            long c = Cos(x);
            if (c == 0) throw new DivideByZeroException("Tangent undefined at pi/2.");
            return DivPrecise(Sin(x), c);
        }

        private static readonly int[] AtanPoly5Lut8Table =
        {
            204464916, 1544566, -357994250, 1395, 1073741820, 0,
            119369854, 56362968, -372884915, 2107694, 1073588633, 4534,
            10771151, 190921163, -440520632, 19339556, 1071365339, 120610,
            -64491917, 329189978, -542756389, 57373179, 1064246365, 656900,
            -89925028, 390367074, -601765924, 85907899, 1057328034, 1329793,
            -80805750, 360696628, -563142238, 60762238, 1065515580, 263159,
            -58345538, 276259197, -435975641, -35140679, 1101731779, -5215389,
            -36116738, 179244146, -266417331, -183483381, 1166696761, -16608596,
            0, 0, 0, 0, 0, 843314857 // Atan(1.0)
        };

        private static readonly int[] RcpPoly4Lut8Table =
        {
            -1148419615, 1319222434, -1610444362, 2147483647,
            -800685973, 1007889397, -1331804245, 1879048187,
            -574360341, 788771120, -1124458315, 1667598402,
            -422615467, 629679090, -964953925, 1496336449,
            -317804473, 510978052, -839211077, 1354580252,
            -243306915, 420235338, -737976898, 1235359781,
            -189035133, 349377484, -655027419, 1133719036,
            -148604318, 293233261, -586071060, 1046158737,
            0, 0, 0, 969969664,
        };

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private static int RcpPoly4Lut8(int a)
        {
            int offset = (a >> 27) * 4;
            int y = Qmul30(a, RcpPoly4Lut8Table[offset + 0]);
            y = Qmul30(a, y + RcpPoly4Lut8Table[offset + 1]);
            y = Qmul30(a, y + RcpPoly4Lut8Table[offset + 2]);
            y = y + RcpPoly4Lut8Table[offset + 3];
            return y;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private static int AtanPoly5Lut8(int a)
        {
            if (a < 0) a = 0;
            int idx = a >> 27;
            if (idx > 8) idx = 8;
            int offset = idx * 6;
            int y = Qmul30(a, AtanPoly5Lut8Table[offset + 0]);
            y = Qmul30(a, y + AtanPoly5Lut8Table[offset + 1]);
            y = Qmul30(a, y + AtanPoly5Lut8Table[offset + 2]);
            y = Qmul30(a, y + AtanPoly5Lut8Table[offset + 3]);
            y = Qmul30(a, y + AtanPoly5Lut8Table[offset + 4]);
            y = y + AtanPoly5Lut8Table[offset + 5];
            return y;
        }

        public const long PiOver4 = 3373259426L; // Pi / 4

        public static long Atan(long z)
        {
            if (z == 0L) return 0L;
            if (z == One) return PiOver4;
            if (z == -One) return -PiOver4;

            bool neg = z < 0L;
            if (neg) z = -z;

            long adjustedResult;
            if (z > One)
            {
                adjustedResult = HalfPi - Atan(DivPrecise(One, z));
            }
            else if (z > Half)
            {
                long transformedZ = DivPrecise(One - z, One + z);
                adjustedResult = PiOver4 - Atan(transformedZ);
            }
            else
            {
                long zSq = Mul(z, z);
                long result = z;
                long term = z;
                long sign = -1L;

                for (int i = 3; i < 15; i += 2)
                {
                    term = Mul(term, zSq);
                    long nextTerm = term / i;
                    if (Abs(nextTerm) <= Epsilon)
                        break;

                    result += nextTerm * sign;
                    sign = -sign;
                }

                adjustedResult = result;
            }

            return neg ? -adjustedResult : adjustedResult;
        }

        public static long Atan2(long y, long x)
        {
            if (x == 0L)
            {
                if (y > 0L) return HalfPi;
                if (y == 0L) return 0L;
                return -HalfPi;
            }

            long atan = Atan(DivPrecise(y, x));

            if (x < 0L)
            {
                if (y >= 0L)
                    return atan + Pi;
                else
                    return atan - Pi;
            }

            return atan;
        }

        public static long Asin(long x)
        {
            if (x < -One || x > One)
                return 0L;
            if (x == One) return HalfPi;
            if (x == -One) return -HalfPi;
            long denom = SqrtPrecise(Mul(One + x, One - x));
            return Atan2(x, denom);
        }

        public static long Acos(long x)
        {
            return HalfPi - Asin(x);
        }

        #endregion

        #region Conversions

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long FromInt(int v) => (long)v << Shift;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long FromFloat(float v) => (long)System.Math.Round((double)v * 4294967296.0);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static long FromDouble(double v) => (long)System.Math.Round(v * 4294967296.0);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static int ToInt(long v) => (int)(v >> Shift);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static float ToFloat(long v) => (float)((double)v / 4294967296.0);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static double ToDouble(long v) => (double)v / 4294967296.0;

        public static string ToString(long v) => ToDouble(v).ToString("0.######", System.Globalization.CultureInfo.InvariantCulture);

        #endregion
    }

    /// <summary>
    /// Fixed64 struct representation. Interchangeable with FP.
    /// </summary>
    [Serializable]
    public struct Fixed64 : IEquatable<Fixed64>, IComparable<Fixed64>
    {
        public long RawValue;

        public static readonly Fixed64 Zero = new Fixed64(Fixed64Math.Zero);
        public static readonly Fixed64 One = new Fixed64(Fixed64Math.One);
        public static readonly Fixed64 Half = new Fixed64(Fixed64Math.Half);
        public static readonly Fixed64 Two = new Fixed64(Fixed64Math.Two);
        public static readonly Fixed64 Pi = new Fixed64(Fixed64Math.Pi);
        public static readonly Fixed64 TwoPi = new Fixed64(Fixed64Math.TwoPi);
        public static readonly Fixed64 HalfPi = new Fixed64(Fixed64Math.HalfPi);
        public static readonly Fixed64 MinValue = new Fixed64(Fixed64Math.MinValue);
        public static readonly Fixed64 MaxValue = new Fixed64(Fixed64Math.MaxValue);
        public static readonly Fixed64 Epsilon = new Fixed64(Fixed64Math.Epsilon);

        public Fixed64(long raw) { RawValue = raw; }
        public Fixed64(int val) { RawValue = Fixed64Math.FromInt(val); }

        public static Fixed64 FromRaw(long raw) => new Fixed64(raw);
        public static Fixed64 FromInt(int val) => new Fixed64(Fixed64Math.FromInt(val));
        public static Fixed64 FromFloat(float val) => new Fixed64(Fixed64Math.FromFloat(val));
        public static Fixed64 FromDouble(double val) => new Fixed64(Fixed64Math.FromDouble(val));

        public static implicit operator Fixed64(int val) => FromInt(val);
        public static explicit operator Fixed64(float val) => FromFloat(val);
        public static explicit operator Fixed64(double val) => FromDouble(val);

        public static explicit operator float(Fixed64 val) => Fixed64Math.ToFloat(val.RawValue);
        public static explicit operator double(Fixed64 val) => Fixed64Math.ToDouble(val.RawValue);
        public static explicit operator int(Fixed64 val) => Fixed64Math.ToInt(val.RawValue);
        public static explicit operator long(Fixed64 val) => val.RawValue >> Fixed64Math.Shift;

        public static implicit operator Fixed64(FP fp) => new Fixed64(fp.RawValue);
        public static implicit operator FP(Fixed64 f64) => new FP(f64.RawValue);

        public static Fixed64 operator +(Fixed64 a, Fixed64 b) => new Fixed64(a.RawValue + b.RawValue);
        public static Fixed64 operator -(Fixed64 a, Fixed64 b) => new Fixed64(a.RawValue - b.RawValue);
        public static Fixed64 operator -(Fixed64 a) => new Fixed64(-a.RawValue);
        public static Fixed64 operator *(Fixed64 a, Fixed64 b) => new Fixed64(Fixed64Math.Mul(a.RawValue, b.RawValue));
        public static Fixed64 operator /(Fixed64 a, Fixed64 b) => new Fixed64(Fixed64Math.DivPrecise(a.RawValue, b.RawValue));
        public static Fixed64 operator %(Fixed64 a, Fixed64 b) => new Fixed64(Fixed64Math.Mod(a.RawValue, b.RawValue));

        public static bool operator ==(Fixed64 a, Fixed64 b) => a.RawValue == b.RawValue;
        public static bool operator !=(Fixed64 a, Fixed64 b) => a.RawValue != b.RawValue;
        public static bool operator <(Fixed64 a, Fixed64 b) => a.RawValue < b.RawValue;
        public static bool operator >(Fixed64 a, Fixed64 b) => a.RawValue > b.RawValue;
        public static bool operator <=(Fixed64 a, Fixed64 b) => a.RawValue <= b.RawValue;
        public static bool operator >=(Fixed64 a, Fixed64 b) => a.RawValue >= b.RawValue;

        public bool Equals(Fixed64 other) => RawValue == other.RawValue;
        public override bool Equals(object obj) => obj is Fixed64 other && Equals(other);
        public override int GetHashCode() => RawValue.GetHashCode();
        public int CompareTo(Fixed64 other) => RawValue.CompareTo(other.RawValue);
        public override string ToString() => Fixed64Math.ToString(RawValue);
    }
}
