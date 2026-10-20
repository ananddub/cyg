//-----------------------------------------------------------------------------
// FPMatrix4x4.cs
// 4x4 Fixed-Point Matrix for deterministic simulation.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Row-major 4x4 fixed-point matrix. Elements are stored as m[row][column],
    /// so m01 is row 0, column 1.
    /// </summary>
    [Serializable]
    public struct FPMatrix4x4 : IEquatable<FPMatrix4x4>, ICYGValue
    {
        public FP m00;
        public FP m01;
        public FP m02;
        public FP m03;
        public FP m10;
        public FP m11;
        public FP m12;
        public FP m13;
        public FP m20;
        public FP m21;
        public FP m22;
        public FP m23;
        public FP m30;
        public FP m31;
        public FP m32;
        public FP m33;

        public static readonly FPMatrix4x4 Identity = new FPMatrix4x4
        {
            m00 = FP.One,
            m11 = FP.One,
            m22 = FP.One,
            m33 = FP.One
        };

        public FPMatrix4x4(
            FP m00, FP m01, FP m02, FP m03,
            FP m10, FP m11, FP m12, FP m13,
            FP m20, FP m21, FP m22, FP m23,
            FP m30, FP m31, FP m32, FP m33)
        {
            this.m00 = m00;
            this.m01 = m01;
            this.m02 = m02;
            this.m03 = m03;
            this.m10 = m10;
            this.m11 = m11;
            this.m12 = m12;
            this.m13 = m13;
            this.m20 = m20;
            this.m21 = m21;
            this.m22 = m22;
            this.m23 = m23;
            this.m30 = m30;
            this.m31 = m31;
            this.m32 = m32;
            this.m33 = m33;
        }

        /// <summary>Row-major indexer; index 0 is m00 and index 15 is m33.</summary>
        public FP this[int index]
        {
            readonly get
            {
                switch (index)
                {
                    case 0: return m00;
                    case 1: return m01;
                    case 2: return m02;
                    case 3: return m03;
                    case 4: return m10;
                    case 5: return m11;
                    case 6: return m12;
                    case 7: return m13;
                    case 8: return m20;
                    case 9: return m21;
                    case 10: return m22;
                    case 11: return m23;
                    case 12: return m30;
                    case 13: return m31;
                    case 14: return m32;
                    case 15: return m33;
                    default: throw new IndexOutOfRangeException();
                }
            }
            set
            {
                switch (index)
                {
                    case 0: m00 = value; break;
                    case 1: m01 = value; break;
                    case 2: m02 = value; break;
                    case 3: m03 = value; break;
                    case 4: m10 = value; break;
                    case 5: m11 = value; break;
                    case 6: m12 = value; break;
                    case 7: m13 = value; break;
                    case 8: m20 = value; break;
                    case 9: m21 = value; break;
                    case 10: m22 = value; break;
                    case 11: m23 = value; break;
                    case 12: m30 = value; break;
                    case 13: m31 = value; break;
                    case 14: m32 = value; break;
                    case 15: m33 = value; break;
                    default: throw new IndexOutOfRangeException();
                }
            }
        }

        public static FPMatrix4x4 operator *(FPMatrix4x4 a, FPMatrix4x4 b)
        {
            FPMatrix4x4 result = default(FPMatrix4x4);
            for (int row = 0; row < 4; row++)
            {
                for (int column = 0; column < 4; column++)
                {
                    FP sum = FP.Zero;
                    for (int i = 0; i < 4; i++)
                    {
                        sum += a[row * 4 + i] * b[i * 4 + column];
                    }
                    result[row * 4 + column] = sum;
                }
            }
            return result;
        }

        public static bool operator ==(FPMatrix4x4 a, FPMatrix4x4 b) => a.Equals(b);

        public static bool operator !=(FPMatrix4x4 a, FPMatrix4x4 b) => !a.Equals(b);

        #region CYG Wire Codec

        public const int SizeInBytes = 128;

        public readonly int WireSize => SizeInBytes;

        public readonly int WriteTo(byte[] buffer, int offset)
        {
            CYGWire.RequireWritable(buffer, offset, SizeInBytes);
            for (int i = 0; i < 16; i++)
            {
                this[i].WriteTo(buffer, offset + (i * FP.SizeInBytes));
            }
            return SizeInBytes;
        }

        public void ReadFrom(byte[] buffer, int offset)
        {
            CYGWire.RequireReadable(buffer, offset, SizeInBytes);
            for (int i = 0; i < 16; i++)
            {
                FP value = default(FP);
                value.ReadFrom(buffer, offset + (i * FP.SizeInBytes));
                this[i] = value;
            }
        }

        public readonly void ComputeChecksum(ref uint crc)
        {
            for (int i = 0; i < 16; i++)
            {
                this[i].ComputeChecksum(ref crc);
            }
        }

        #endregion

        #region Interfaces

        public readonly bool Equals(FPMatrix4x4 other)
        {
            for (int i = 0; i < 16; i++)
            {
                if (this[i] != other[i])
                {
                    return false;
                }
            }
            return true;
        }

        public readonly override bool Equals(object obj) => obj is FPMatrix4x4 other && Equals(other);

        public readonly override int GetHashCode()
        {
            unchecked
            {
                int hash = 17;
                for (int i = 0; i < 16; i++)
                {
                    hash = (hash * 31) ^ this[i].GetHashCode();
                }
                return hash;
            }
        }

        public readonly override string ToString()
        {
            return "[" + m00 + ", " + m01 + ", " + m02 + ", " + m03 + "; "
                + m10 + ", " + m11 + ", " + m12 + ", " + m13 + "; "
                + m20 + ", " + m21 + ", " + m22 + ", " + m23 + "; "
                + m30 + ", " + m31 + ", " + m32 + ", " + m33 + "]";
        }

        #endregion
    }
}
