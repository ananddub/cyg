//-----------------------------------------------------------------------------
// FPQuaternion.cs
// 3D Fixed-Point Quaternion for deterministic simulation.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Deterministic 3D fixed-point quaternion.
    /// </summary>
    [Serializable]
    public struct FPQuaternion : IEquatable<FPQuaternion>
    {
        public FP x;
        public FP y;
        public FP z;
        public FP w;

        public FP X { [MethodImpl(MethodImplOptions.AggressiveInlining)] get => x; set => x = value; }
        public FP Y { [MethodImpl(MethodImplOptions.AggressiveInlining)] get => y; set => y = value; }
        public FP Z { [MethodImpl(MethodImplOptions.AggressiveInlining)] get => z; set => z = value; }
        public FP W { [MethodImpl(MethodImplOptions.AggressiveInlining)] get => w; set => w = value; }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPQuaternion(FP x, FP y, FP z, FP w)
        {
            this.x = x;
            this.y = y;
            this.z = z;
            this.w = w;
        }

        #region Constants

        public static readonly FPQuaternion Identity = new FPQuaternion(FP.Zero, FP.Zero, FP.Zero, FP.One);

        #endregion

        #region Properties & Methods

        public FP SqrMagnitude
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => x * x + y * y + z * z + w * w;
        }

        public FP Magnitude
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => FP.Sqrt(SqrMagnitude);
        }

        public FPQuaternion Normalized
        {
            get
            {
                FP mag = Magnitude;
                if (mag <= FP.Zero)
                    return Identity;
                FP invMag = FP.One / mag;
                return new FPQuaternion(x * invMag, y * invMag, z * invMag, w * invMag);
            }
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void Normalize()
        {
            this = Normalized;
        }

        public FPQuaternion Conjugated
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => new FPQuaternion(-x, -y, -z, w);
        }

        public FPQuaternion Inverse
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => Conjugated.Normalized;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP Dot(FPQuaternion a, FPQuaternion b) => a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;

        /// <summary>
        /// Creates a rotation quaternion from an axis and angle in radians.
        /// </summary>
        public static FPQuaternion FromAxisAngle(FPVector3 axis, FP angleRadians)
        {
            FP sqrMag = axis.SqrMagnitude;
            if (sqrMag < FP.Epsilon)
                return Identity;

            FPVector3 normAxis = axis.Normalized;
            FP halfAngle = angleRadians * FP.Half;
            FP sin = FP.Sin(halfAngle);
            FP cos = FP.Cos(halfAngle);

            return new FPQuaternion(normAxis.x * sin, normAxis.y * sin, normAxis.z * sin, cos).Normalized;
        }

        /// <summary>
        /// Creates a rotation quaternion from an axis and angle in degrees (matches Unity AngleAxis).
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPQuaternion AngleAxis(FP angleDegrees, FPVector3 axis)
        {
            return FromAxisAngle(axis, angleDegrees * FP.Deg2Rad);
        }

        /// <summary>
        /// Returns a rotation that rotates z degrees around the z axis, x degrees around the x axis,
        /// and y degrees around the y axis (in that order). Input in degrees.
        /// </summary>
        public static FPQuaternion Euler(FP pitchX, FP yawY, FP rollZ)
        {
            FP halfX = (pitchX * FP.Deg2Rad) * FP.Half;
            FP halfY = (yawY * FP.Deg2Rad) * FP.Half;
            FP halfZ = (rollZ * FP.Deg2Rad) * FP.Half;

            FP sx = FP.Sin(halfX);
            FP cx = FP.Cos(halfX);
            FP sy = FP.Sin(halfY);
            FP cy = FP.Cos(halfY);
            FP sz = FP.Sin(halfZ);
            FP cz = FP.Cos(halfZ);

            // Order YXZ: q = qY * qX * qZ
            FP qx = cy * sx * cz + sy * cx * sz;
            FP qy = sy * cx * cz - cy * sx * sz;
            FP qz = cy * cx * sz - sy * sx * cz;
            FP qw = cy * cx * cz + sy * sx * sz;

            return new FPQuaternion(qx, qy, qz, qw).Normalized;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPQuaternion Euler(FPVector3 eulerDegrees) => Euler(eulerDegrees.x, eulerDegrees.y, eulerDegrees.z);

        /// <summary>
        /// Converts quaternion back to Euler angles in degrees (pitch X, yaw Y, roll Z).
        /// </summary>
        public FPVector3 ToEuler()
        {
            FP sinP = FP.FromInt(2) * (w * x - y * z);
            FP pitch, yaw, roll;

            if (FP.Abs(sinP) >= FP.One - (FP)0.001f)
            {
                pitch = FP.Sign(sinP) > 0 ? (FP)90 : (FP)270;
                yaw = FP.FromInt(2) * FP.Atan2(y, w) * FP.Rad2Deg;
                roll = FP.Zero;
            }
            else
            {
                pitch = FP.Asin(sinP) * FP.Rad2Deg;
                yaw = FP.Atan2(FP.FromInt(2) * (w * y + z * x), FP.One - FP.FromInt(2) * (x * x + y * y)) * FP.Rad2Deg;
                roll = FP.Atan2(FP.FromInt(2) * (w * z + x * y), FP.One - FP.FromInt(2) * (x * x + z * z)) * FP.Rad2Deg;
            }

            if (pitch < FP.Zero) pitch += (FP)360;
            if (yaw < FP.Zero) yaw += (FP)360;
            if (roll < FP.Zero) roll += (FP)360;

            return new FPVector3(pitch, yaw, roll);
        }

        public FPVector3 EulerAngles => ToEuler();

        /// <summary>
        /// Spherical linear interpolation between two quaternions.
        /// </summary>
        public static FPQuaternion Slerp(FPQuaternion a, FPQuaternion b, FP t)
        {
            FP cosOmega = Dot(a, b);
            FP sign = FP.One;

            if (cosOmega < FP.Zero)
            {
                cosOmega = -cosOmega;
                sign = -FP.One;
            }

            FP s1, s2;
            if (cosOmega > FP.One - (FP)0.0001f)
            {
                // Too close: fall back to linear interpolation
                s1 = FP.One - t;
                s2 = t * sign;
            }
            else
            {
                FP omega = FP.Acos(cosOmega);
                FP sinOmega = FP.Sin(omega);
                if (sinOmega < FP.Epsilon)
                {
                    s1 = FP.One - t;
                    s2 = t * sign;
                }
                else
                {
                    FP invSinOmega = FP.One / sinOmega;
                    s1 = FP.Sin((FP.One - t) * omega) * invSinOmega;
                    s2 = FP.Sin(t * omega) * invSinOmega * sign;
                }
            }

            return new FPQuaternion(
                s1 * a.x + s2 * b.x,
                s1 * a.y + s2 * b.y,
                s1 * a.z + s2 * b.z,
                s1 * a.w + s2 * b.w
            ).Normalized;
        }

        /// <summary>
        /// Regular linear interpolation normalized.
        /// </summary>
        public static FPQuaternion Lerp(FPQuaternion a, FPQuaternion b, FP t)
        {
            FP t1 = FP.One - t;
            FP dot = Dot(a, b);
            FP sign = dot >= FP.Zero ? FP.One : -FP.One;

            return new FPQuaternion(
                t1 * a.x + sign * t * b.x,
                t1 * a.y + sign * t * b.y,
                t1 * a.z + sign * t * b.z,
                t1 * a.w + sign * t * b.w
            ).Normalized;
        }

        /// <summary>
        /// Multiplies two quaternions (concatenates rotations).
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPQuaternion Multiply(FPQuaternion a, FPQuaternion b)
        {
            return new FPQuaternion(
                a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
                a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
                a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
                a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z
            );
        }

        /// <summary>
        /// Rotates vector by quaternion.
        /// </summary>
        public static FPVector3 Rotate(FPQuaternion rotation, FPVector3 point)
        {
            FPVector3 u = new FPVector3(rotation.x, rotation.y, rotation.z);
            FP s = rotation.w;

            return (FP.FromInt(2) * FPVector3.Dot(u, point)) * u
                 + (s * s - FPVector3.Dot(u, u)) * point
                 + (FP.FromInt(2) * s) * FPVector3.Cross(u, point);
        }

        /// <summary>
        /// Creates a rotation with the specified forward and upwards directions.
        /// </summary>
        public static FPQuaternion LookRotation(FPVector3 forward, FPVector3 upwards)
        {
            FP sqrMag = forward.SqrMagnitude;
            if (sqrMag < FP.Epsilon)
                return Identity;

            FPVector3 f = forward.Normalized;
            FPVector3 r = FPVector3.Cross(upwards, f);
            if (r.SqrMagnitude < FP.Epsilon)
                r = FPVector3.Cross(FPVector3.Up, f);
            if (r.SqrMagnitude < FP.Epsilon)
                r = FPVector3.Cross(FPVector3.Right, f);

            r.Normalize();
            FPVector3 u = FPVector3.Cross(f, r);

            // Shepperd's algorithm
            FP m00 = r.x, m01 = u.x, m02 = f.x;
            FP m10 = r.y, m11 = u.y, m12 = f.y;
            FP m20 = r.z, m21 = u.z, m22 = f.z;

            FP tr = m00 + m11 + m22;
            FP qx, qy, qz, qw;

            if (tr > FP.Zero)
            {
                FP s = FP.Sqrt(tr + FP.One) * FP.FromInt(2);
                qw = (FP)0.25f * s;
                qx = (m21 - m12) / s;
                qy = (m02 - m20) / s;
                qz = (m10 - m01) / s;
            }
            else if (m00 > m11 && m00 > m22)
            {
                FP s = FP.Sqrt(FP.One + m00 - m11 - m22) * FP.FromInt(2);
                qw = (m21 - m12) / s;
                qx = (FP)0.25f * s;
                qy = (m01 + m10) / s;
                qz = (m02 + m20) / s;
            }
            else if (m11 > m22)
            {
                FP s = FP.Sqrt(FP.One + m11 - m00 - m22) * FP.FromInt(2);
                qw = (m02 - m20) / s;
                qx = (m01 + m10) / s;
                qy = (FP)0.25f * s;
                qz = (m12 + m21) / s;
            }
            else
            {
                FP s = FP.Sqrt(FP.One + m22 - m00 - m11) * FP.FromInt(2);
                qw = (m10 - m01) / s;
                qx = (m02 + m20) / s;
                qy = (m12 + m21) / s;
                qz = (FP)0.25f * s;
            }

            return new FPQuaternion(qx, qy, qz, qw).Normalized;
        }

        public static FPQuaternion LookRotation(FPVector3 forward) => LookRotation(forward, FPVector3.Up);

        public static FPQuaternion FromToRotation(FPVector3 fromDirection, FPVector3 toDirection)
        {
            FPVector3 f = fromDirection.Normalized;
            FPVector3 t = toDirection.Normalized;

            FP dot = FPVector3.Dot(f, t);
            if (dot >= FP.One - (FP)0.0001f)
                return Identity;

            if (dot <= -FP.One + (FP)0.0001f)
            {
                FPVector3 axis = FPVector3.Cross(FPVector3.Right, f);
                if (axis.SqrMagnitude < (FP)0.001f)
                    axis = FPVector3.Cross(FPVector3.Up, f);
                return FromAxisAngle(axis.Normalized, FP.Pi);
            }

            FPVector3 cross = FPVector3.Cross(f, t);
            FP w = FP.Sqrt((FP.One + dot) * FP.FromInt(2));
            FP invW = FP.One / w;
            return new FPQuaternion(cross.x * invW, cross.y * invW, cross.z * invW, w * FP.Half).Normalized;
        }

        public static FP Angle(FPQuaternion a, FPQuaternion b)
        {
            FP dot = FP.Abs(Dot(a, b));
            if (dot > FP.One) dot = FP.One;
            return FP.Acos(dot) * FP.FromInt(2) * FP.Rad2Deg;
        }

        #endregion

        #region Operators

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPQuaternion operator *(FPQuaternion a, FPQuaternion b) => Multiply(a, b);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 operator *(FPQuaternion rotation, FPVector3 point) => Rotate(rotation, point);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static bool operator ==(FPQuaternion a, FPQuaternion b) => a.x == b.x && a.y == b.y && a.z == b.z && a.w == b.w;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static bool operator !=(FPQuaternion a, FPQuaternion b) => !(a == b);

        #endregion

        #region Interfaces

        public bool Equals(FPQuaternion other) => x == other.x && y == other.y && z == other.z && w == other.w;
        public override bool Equals(object obj) => obj is FPQuaternion other && Equals(other);
        public override int GetHashCode() => x.GetHashCode() ^ (y.GetHashCode() * 397) ^ (z.GetHashCode() * 7919) ^ (w.GetHashCode() * 4513);
        public override string ToString() => $"({x}, {y}, {z}, {w})";

        #endregion
    }
}
