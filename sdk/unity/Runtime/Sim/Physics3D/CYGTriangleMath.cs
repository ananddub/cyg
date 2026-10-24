using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Fixed-point triangle primitives shared by the static mesh query and cast
    /// code. Kept separate so the query/cast files stay readable.
    /// </summary>
    internal static class CYGTriangleMath
    {
        internal static readonly FP Epsilon = FP.FromFraction(1, 1000000);

        /// <summary>Ericson's closest-point-on-triangle, restricted to FP math.</summary>
        internal static FPVector3 ClosestPointOnTriangle(
            FPVector3 point,
            FPVector3 a,
            FPVector3 b,
            FPVector3 c)
        {
            FPVector3 ab = b - a;
            FPVector3 ac = c - a;
            FPVector3 ap = point - a;

            FP d1 = FPVector3.Dot(ab, ap);
            FP d2 = FPVector3.Dot(ac, ap);
            if (d1 <= FP.Zero && d2 <= FP.Zero)
            {
                return a;
            }

            FPVector3 bp = point - b;
            FP d3 = FPVector3.Dot(ab, bp);
            FP d4 = FPVector3.Dot(ac, bp);
            if (d3 >= FP.Zero && d4 <= d3)
            {
                return b;
            }

            FP vc = d1 * d4 - d3 * d2;
            if (vc <= FP.Zero && d1 >= FP.Zero && d3 <= FP.Zero)
            {
                FP denominator = d1 - d3;
                return a + ab * (FP.Abs(denominator) > Epsilon ? d1 / denominator : FP.Zero);
            }

            FPVector3 cp = point - c;
            FP d5 = FPVector3.Dot(ab, cp);
            FP d6 = FPVector3.Dot(ac, cp);
            if (d6 >= FP.Zero && d5 <= d6)
            {
                return c;
            }

            FP vb = d5 * d2 - d1 * d6;
            if (vb <= FP.Zero && d2 >= FP.Zero && d6 <= FP.Zero)
            {
                FP denominator = d2 - d6;
                return a + ac * (FP.Abs(denominator) > Epsilon ? d2 / denominator : FP.Zero);
            }

            FP va = d3 * d6 - d5 * d4;
            if (va <= FP.Zero && d4 - d3 >= FP.Zero && d5 - d6 >= FP.Zero)
            {
                FP denominator = (d4 - d3) + (d5 - d6);
                FP w = FP.Abs(denominator) > Epsilon ? (d4 - d3) / denominator : FP.Zero;
                return b + (c - b) * w;
            }

            FP total = va + vb + vc;
            if (FP.Abs(total) <= Epsilon)
            {
                return a;
            }

            FP inverse = FP.One / total;
            FP baryV = vb * inverse;
            FP baryW = vc * inverse;
            return a + ab * baryV + ac * baryW;
        }

        /// <summary>Moller-Trumbore ray/triangle intersection with a fixed-point epsilon.</summary>
        internal static bool RayTriangle(
            FPVector3 origin,
            FPVector3 direction,
            FPVector3 a,
            FPVector3 b,
            FPVector3 c,
            FP maxDistance,
            out FP distance,
            out FPVector3 point,
            out FPVector3 normal)
        {
            distance = FP.Zero;
            point = origin;
            normal = FPVector3.Up;

            FPVector3 edge1 = b - a;
            FPVector3 edge2 = c - a;
            FPVector3 p = FPVector3.Cross(direction, edge2);
            FP determinant = FPVector3.Dot(edge1, p);
            if (FP.Abs(determinant) <= Epsilon)
            {
                return false;
            }

            FP inverse = FP.One / determinant;
            FPVector3 t = origin - a;
            FP u = FPVector3.Dot(t, p) * inverse;
            if (u < FP.Zero || u > FP.One)
            {
                return false;
            }

            FPVector3 q = FPVector3.Cross(t, edge1);
            FP v = FPVector3.Dot(direction, q) * inverse;
            if (v < FP.Zero || u + v > FP.One)
            {
                return false;
            }

            FP hitDistance = FPVector3.Dot(edge2, q) * inverse;
            if (hitDistance < Epsilon || hitDistance > maxDistance)
            {
                return false;
            }

            distance = hitDistance;
            point = origin + direction * hitDistance;
            normal = FaceNormal(a, b, c, direction);
            return true;
        }

        internal static FPVector3 FaceNormal(
            FPVector3 a,
            FPVector3 b,
            FPVector3 c,
            FPVector3 direction)
        {
            FPVector3 cross = FPVector3.Cross(b - a, c - a);
            FP length = cross.Magnitude;
            if (length <= Epsilon)
            {
                return FPVector3.Up;
            }

            FPVector3 normal = cross / length;
            return FPVector3.Dot(normal, direction) > FP.Zero ? -normal : normal;
        }

        internal static FP Fraction(FP distance, FP maxDistance)
        {
            return maxDistance > FP.Zero ? FP.Clamp01(distance / maxDistance) : FP.Zero;
        }
    }
}
