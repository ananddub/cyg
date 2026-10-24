//-----------------------------------------------------------------------------
// CYGOrientedBoxSAT.cs
// Oriented-box SAT face/edge tests and edge-contact generation.
// Zero UnityEngine dependencies. All math is Q32.32 fixed-point.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    internal static partial class CYGOrientedBox
    {
        public static bool BoxBox(
            CYGPhysicsBody a,
            CYGPhysicsBody b,
            FPVector3 centerA,
            FPVector3 centerB,
            out CYGManifold manifold)
        {
            manifold = default(CYGManifold);

            GetAxes(a.Rotation, out FPVector3 ax, out FPVector3 ay, out FPVector3 az);
            GetAxes(b.Rotation, out FPVector3 bx, out FPVector3 by, out FPVector3 bz);
            FPVector3 halfA = a.Collider.Shape.HalfExtents;
            FPVector3 halfB = b.Collider.Shape.HalfExtents;
            FPVector3 delta = centerB - centerA;

            FPVector3 a0 = ax, a1 = ay, a2 = az;
            FPVector3 b0 = bx, b1 = by, b2 = bz;

            if (!TestFaceAxes(a0, a1, a2, halfA, b0, b1, b2, halfB, delta, 0, out FaceTest bestFaceA, out bool separatedA))
                return false;
            if (!TestFaceAxes(b0, b1, b2, halfB, a0, a1, a2, halfA, delta, 1, out FaceTest bestFaceB, out bool separatedB))
                return false;
            if (separatedA || separatedB)
                return false;

            FaceTest bestFace = bestFaceB.Overlap < bestFaceA.Overlap ? bestFaceB : bestFaceA;

            EdgeTest bestEdge = default(EdgeTest);
            bestEdge.Overlap = FP.MaxValue;
            bool hasEdge = false;
            if (!TestEdges(a0, a1, a2, halfA, b0, b1, b2, halfB, delta, ref bestEdge, ref hasEdge))
                return false;
            if (hasEdge && bestEdge.Overlap < bestFace.Overlap - FP.FromFraction(1, 50))
            {
                FPVector3 normal = Orient(bestEdge.Axis, delta);
                FPVector3 point = EdgeContactPoint(a, b, centerA, centerB, normal, bestEdge, halfA, halfB);
                manifold = Create(a, b, normal, point, bestEdge.Overlap);
                return true;
            }

            FPVector3 faceNormal = Orient(bestFace.Axis, delta);
            if (bestFace.Body == 0)
            {
                BuildFaceManifold(
                    a, b, centerA, centerB,
                    ax, ay, az, halfA, bx, by, bz, halfB,
                    centerA, centerB, faceNormal, faceNormal, out manifold);
            }
            else
            {
                BuildFaceManifold(
                    a, b, centerA, centerB,
                    bx, by, bz, halfB, ax, ay, az, halfA,
                    centerB, centerA, -faceNormal, faceNormal, out manifold);
            }

            if (manifold.PointCount > 0)
                return true;

            // Degenerate clipping (parallel/tiny faces): keep the SAT result.
            manifold = Create(a, b, faceNormal, (centerA + centerB) * FP.Half, bestFace.Overlap);
            return true;
        }

        private struct FaceTest
        {
            public FP Overlap;
            public FPVector3 Axis;
            public int Body;
            public int Index;
        }

        private struct EdgeTest
        {
            public FP Overlap;
            public FPVector3 Axis;
            public int IndexA;
            public int IndexB;
        }

        private static bool TestFaceAxes(
            FPVector3 x,
            FPVector3 y,
            FPVector3 z,
            FPVector3 half,
            FPVector3 otherX,
            FPVector3 otherY,
            FPVector3 otherZ,
            FPVector3 otherHalf,
            FPVector3 delta,
            int body,
            out FaceTest best,
            out bool separated)
        {
            best = default(FaceTest);
            best.Overlap = FP.MaxValue;
            separated = false;

            if (!TestFaceAxis(x, x, y, z, half, otherX, otherY, otherZ, otherHalf, delta, 0, body, ref best))
            {
                separated = true;
                return false;
            }
            if (!TestFaceAxis(y, x, y, z, half, otherX, otherY, otherZ, otherHalf, delta, 1, body, ref best))
            {
                separated = true;
                return false;
            }
            if (!TestFaceAxis(z, x, y, z, half, otherX, otherY, otherZ, otherHalf, delta, 2, body, ref best))
            {
                separated = true;
                return false;
            }
            return true;
        }

        private static bool TestFaceAxis(
            FPVector3 axis,
            FPVector3 x,
            FPVector3 y,
            FPVector3 z,
            FPVector3 half,
            FPVector3 otherX,
            FPVector3 otherY,
            FPVector3 otherZ,
            FPVector3 otherHalf,
            FPVector3 delta,
            int index,
            int body,
            ref FaceTest best)
        {
            FP radiusA = Project(axis, x, y, z, half);
            FP radiusB = Project(axis, otherX, otherY, otherZ, otherHalf);
            FP overlap = radiusA + radiusB - FP.Abs(FPVector3.Dot(delta, axis));
            if (overlap < -Epsilon)
                return false;
            Consider(body, index, axis, overlap, ref best);
            return true;
        }

        private static FP Project(FPVector3 axis, FPVector3 x, FPVector3 y, FPVector3 z, FPVector3 half)
        {
            return FP.Abs(FPVector3.Dot(axis, x)) * half.x
                + FP.Abs(FPVector3.Dot(axis, y)) * half.y
                + FP.Abs(FPVector3.Dot(axis, z)) * half.z;
        }

        private static void Consider(int body, int index, FPVector3 axis, FP overlap, ref FaceTest best)
        {
            if (overlap < best.Overlap)
            {
                best.Overlap = overlap;
                best.Axis = axis;
                best.Body = body;
                best.Index = index;
            }
        }

        private static bool TestEdges(
            FPVector3 a0,
            FPVector3 a1,
            FPVector3 a2,
            FPVector3 halfA,
            FPVector3 b0,
            FPVector3 b1,
            FPVector3 b2,
            FPVector3 halfB,
            FPVector3 delta,
            ref EdgeTest best,
            ref bool hasEdge)
        {
            for (int i = 0; i < 3; i++)
            {
                FPVector3 ai = i == 0 ? a0 : i == 1 ? a1 : a2;
                for (int j = 0; j < 3; j++)
                {
                    FPVector3 bj = j == 0 ? b0 : j == 1 ? b1 : b2;
                    FPVector3 cross = FPVector3.Cross(ai, bj);
                    FP length = cross.Magnitude;
                    if (length <= FP.FromFraction(1, 1000))
                        continue;

                    FPVector3 axis = cross / length;
                    FP radiusA = FP.Abs(FPVector3.Dot(axis, a0)) * halfA.x
                        + FP.Abs(FPVector3.Dot(axis, a1)) * halfA.y
                        + FP.Abs(FPVector3.Dot(axis, a2)) * halfA.z;
                    FP radiusB = FP.Abs(FPVector3.Dot(axis, b0)) * halfB.x
                        + FP.Abs(FPVector3.Dot(axis, b1)) * halfB.y
                        + FP.Abs(FPVector3.Dot(axis, b2)) * halfB.z;
                    FP overlap = radiusA + radiusB - FP.Abs(FPVector3.Dot(delta, axis));
                    if (overlap < -Epsilon)
                        return false;

                    if (!hasEdge || overlap < best.Overlap)
                    {
                        best.Overlap = overlap;
                        best.Axis = axis;
                        best.IndexA = i;
                        best.IndexB = j;
                        hasEdge = true;
                    }
                }
            }
            return true;
        }

        private static FPVector3 Orient(FPVector3 axis, FPVector3 delta)
        {
            return FPVector3.Dot(delta, axis) >= FP.Zero ? axis : -axis;
        }

        private static FPVector3 EdgeContactPoint(
            CYGPhysicsBody a,
            CYGPhysicsBody b,
            FPVector3 centerA,
            FPVector3 centerB,
            FPVector3 normal,
            EdgeTest edge,
            FPVector3 halfA,
            FPVector3 halfB)
        {
            FPVector3 axisA = GetAxis(a.Rotation, edge.IndexA);
            FPVector3 axisB = GetAxis(b.Rotation, edge.IndexB);
            FPVector3 offsetA = EdgeOffset(a.Rotation, halfA, edge.IndexA, normal);
            FPVector3 offsetB = EdgeOffset(b.Rotation, halfB, edge.IndexB, -normal);
            FP lenA = GetExtent(halfA, edge.IndexA);
            FP lenB = GetExtent(halfB, edge.IndexB);

            FPVector3 a0 = centerA + offsetA - axisA * lenA;
            FPVector3 a1 = centerA + offsetA + axisA * lenA;
            FPVector3 b0 = centerB + offsetB - axisB * lenB;
            FPVector3 b1 = centerB + offsetB + axisB * lenB;

            FPCapsule.ClosestPointsBetweenSegments(a0, a1, b0, b1, out FPVector3 closestA, out FPVector3 closestB);
            return (closestA + closestB) * FP.Half;
        }

        private static FPVector3 EdgeOffset(FPQuaternion rotation, FPVector3 half, int skipAxis, FPVector3 direction)
        {
            FPVector3 result = FPVector3.Zero;
            for (int axis = 0; axis < 3; axis++)
            {
                if (axis == skipAxis)
                    continue;
                FPVector3 unit = GetAxis(rotation, axis);
                FP sign = FPVector3.Dot(unit, direction) >= FP.Zero ? FP.One : -FP.One;
                result += unit * (GetExtent(half, axis) * sign);
            }
            return result;
        }
    }
}
