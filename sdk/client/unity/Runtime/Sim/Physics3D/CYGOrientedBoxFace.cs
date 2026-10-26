//-----------------------------------------------------------------------------
// CYGOrientedBoxFace.cs
// Rotated face clipping, side clipping and face manifolds.
// Zero UnityEngine dependencies. All math is Q32.32 fixed-point.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    internal static partial class CYGOrientedBox
    {
        private const int MaxPolygonPoints = 8;
        private static readonly FP ContactTolerance = FP.FromFraction(1, 100);

        private static void BuildFaceManifold(
            CYGPhysicsBody a,
            CYGPhysicsBody b,
            FPVector3 centerA,
            FPVector3 centerB,
            FPVector3 refX,
            FPVector3 refY,
            FPVector3 refZ,
            FPVector3 refHalf,
            FPVector3 incX,
            FPVector3 incY,
            FPVector3 incZ,
            FPVector3 incHalf,
            FPVector3 refCenter,
            FPVector3 incCenter,
            FPVector3 referenceToIncident,
            FPVector3 manifoldNormal,
            out CYGManifold manifold)
        {
            manifold = default(CYGManifold);
            manifold.BodyA = a.Id;
            manifold.BodyB = b.Id;

            FPVector3 refFaceCenter;
            FPVector3 sideU;
            FPVector3 sideV;
            FP extentU;
            FP extentV;
            if (!SelectFace(refX, refY, refZ, refHalf, referenceToIncident, out refFaceCenter, out sideU, out sideV, out extentU, out extentV))
                return;
            refFaceCenter += refCenter;

            FPVector3 incFaceCenter;
            FPVector3 incU;
            FPVector3 incV;
            FP incUextent;
            FP incVextent;
            if (!SelectFace(incX, incY, incZ, incHalf, -referenceToIncident, out incFaceCenter, out incU, out incV, out incUextent, out incVextent))
                return;
            incFaceCenter += incCenter;

            Polygon polygon = default(Polygon);
            polygon.Add(incFaceCenter + incU * incUextent + incV * incVextent);
            polygon.Add(incFaceCenter - incU * incUextent + incV * incVextent);
            polygon.Add(incFaceCenter - incU * incUextent - incV * incVextent);
            polygon.Add(incFaceCenter + incU * incUextent - incV * incVextent);

            Clip(ref polygon, refFaceCenter, sideU, extentU);
            Clip(ref polygon, refFaceCenter, -sideU, extentU);
            Clip(ref polygon, refFaceCenter, sideV, extentV);
            Clip(ref polygon, refFaceCenter, -sideV, extentV);

            FPVector3 normal = manifoldNormal;
            for (int i = 0; i < polygon.Count; i++)
            {
                FPVector3 vertex = polygon.Get(i);
                FP separation = FPVector3.Dot(vertex - refFaceCenter, referenceToIncident);
                if (separation > ContactTolerance)
                    continue;

                FPVector3 point = vertex - referenceToIncident * separation;
                manifold.AddPoint(point, FP.Max(FP.Zero, -separation));
            }

            if (manifold.PointCount > 0)
                manifold.Normal = normal;
        }

        private static bool SelectFace(
            FPVector3 x,
            FPVector3 y,
            FPVector3 z,
            FPVector3 half,
            FPVector3 direction,
            out FPVector3 faceCenter,
            out FPVector3 sideU,
            out FPVector3 sideV,
            out FP extentU,
            out FP extentV)
        {
            FP dotX = FPVector3.Dot(x, direction);
            FP dotY = FPVector3.Dot(y, direction);
            FP dotZ = FPVector3.Dot(z, direction);

            int best = 0;
            FP bestDot = dotX;
            FP bestMagnitude = FP.Abs(dotX);
            if (FP.Abs(dotY) > bestMagnitude) { best = 1; bestDot = dotY; bestMagnitude = FP.Abs(dotY); }
            if (FP.Abs(dotZ) > bestMagnitude) { best = 2; bestDot = dotZ; }

            FP sign = bestDot >= FP.Zero ? FP.One : -FP.One;
            if (best == 0)
            {
                faceCenter = x * (half.x * sign);
                sideU = y;
                sideV = z;
                extentU = half.y;
                extentV = half.z;
            }
            else if (best == 1)
            {
                faceCenter = y * (half.y * sign);
                sideU = x;
                sideV = z;
                extentU = half.x;
                extentV = half.z;
            }
            else
            {
                faceCenter = z * (half.z * sign);
                sideU = x;
                sideV = y;
                extentU = half.x;
                extentV = half.y;
            }
            return true;
        }

        private static void Clip(ref Polygon polygon, FPVector3 refCenter, FPVector3 planeNormal, FP limit)
        {
            if (polygon.Count == 0)
                return;

            Polygon output = default(Polygon);
            for (int i = 0; i < polygon.Count; i++)
            {
                FPVector3 current = polygon.Get(i);
                FPVector3 next = polygon.Get((i + 1) % polygon.Count);
                FP currentDistance = FPVector3.Dot(current - refCenter, planeNormal) - limit;
                FP nextDistance = FPVector3.Dot(next - refCenter, planeNormal) - limit;
                bool currentInside = currentDistance <= FP.Zero;
                bool nextInside = nextDistance <= FP.Zero;

                if (currentInside)
                    output.Add(current);
                if (currentInside != nextInside)
                {
                    FP denominator = currentDistance - nextDistance;
                    if (FP.Abs(denominator) > Epsilon)
                    {
                        FP t = currentDistance / denominator;
                        output.Add(current + (next - current) * t);
                    }
                }
            }

            polygon = output;
        }

        private static CYGManifold Create(
            CYGPhysicsBody a,
            CYGPhysicsBody b,
            FPVector3 normal,
            FPVector3 point,
            FP depth)
        {
            CYGManifold manifold = default(CYGManifold);
            manifold.BodyA = a.Id;
            manifold.BodyB = b.Id;
            manifold.Normal = normal;
            manifold.AddPoint(point, depth);
            return manifold;
        }

        private struct Polygon
        {
            public int Count;
            private FPVector3 _p0;
            private FPVector3 _p1;
            private FPVector3 _p2;
            private FPVector3 _p3;
            private FPVector3 _p4;
            private FPVector3 _p5;
            private FPVector3 _p6;
            private FPVector3 _p7;

            public void Add(FPVector3 point)
            {
                if (Count >= MaxPolygonPoints)
                    return;
                switch (Count)
                {
                    case 0: _p0 = point; break;
                    case 1: _p1 = point; break;
                    case 2: _p2 = point; break;
                    case 3: _p3 = point; break;
                    case 4: _p4 = point; break;
                    case 5: _p5 = point; break;
                    case 6: _p6 = point; break;
                    default: _p7 = point; break;
                }
                Count++;
            }

            public FPVector3 Get(int index)
            {
                switch (index)
                {
                    case 0: return _p0;
                    case 1: return _p1;
                    case 2: return _p2;
                    case 3: return _p3;
                    case 4: return _p4;
                    case 5: return _p5;
                    case 6: return _p6;
                    default: return _p7;
                }
            }
        }
    }
}
