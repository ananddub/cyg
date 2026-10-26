//-----------------------------------------------------------------------------
// CYGPhysics2DManifold.cs
// Fixed-point narrowphase: circle-circle, circle-OBB, OBB-OBB SAT with clipping,
// plus deterministic raycast and overlap helpers.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    internal static class CYGPhysics2DMath
    {
        public static FPVector2 Rotate(FP angle, FPVector2 local)
        {
            FP cos = FP.Cos(angle);
            FP sin = FP.Sin(angle);
            return new FPVector2(cos * local.x - sin * local.y, sin * local.x + cos * local.y);
        }

        public static FPVector2 Rotate(FP cos, FP sin, FPVector2 local)
        {
            return new FPVector2(cos * local.x - sin * local.y, sin * local.x + cos * local.y);
        }

        public static FP Cross(FPVector2 a, FPVector2 b)
        {
            return a.x * b.y - a.y * b.x;
        }

        public static FPVector2 Cross(FP scalar, FPVector2 value)
        {
            return new FPVector2(-scalar * value.y, scalar * value.x);
        }

        /// <summary>Wraps an angle into [-Pi, Pi] without floating point.</summary>
        public static FP NormalizeAngle(FP angle)
        {
            if (angle > FP.Pi || angle < -FP.Pi)
            {
                angle = angle % FP.TwoPi;
                if (angle > FP.Pi)
                {
                    angle -= FP.TwoPi;
                }
                else if (angle < -FP.Pi)
                {
                    angle += FP.TwoPi;
                }
            }

            return angle;
        }
    }

    /// <summary>Oriented box in world space with cached unit axes.</summary>
    internal struct CYGPhysics2DOBB
    {
        public FPVector2 Center;
        public FPVector2 Right;
        public FPVector2 Up;
        public FPVector2 HalfExtents;

        public static CYGPhysics2DOBB Create(in CYGPhysics2DBody body)
        {
            FP cos = FP.Cos(body.Angle);
            FP sin = FP.Sin(body.Angle);
            CYGPhysics2DOBB obb;
            obb.Center = body.Position;
            obb.Right = new FPVector2(cos, sin);
            obb.Up = new FPVector2(-sin, cos);
            obb.HalfExtents = body.HalfExtents;
            return obb;
        }

        public FPVector2 ToWorld(FP localX, FP localY)
        {
            return Center + Right * localX + Up * localY;
        }

        public FPVector2 ToWorld(FPVector2 local)
        {
            return Center + Right * local.x + Up * local.y;
        }

        public FPVector2 DirectionToWorld(FPVector2 local)
        {
            return Right * local.x + Up * local.y;
        }

        public FPVector2 ToLocal(FPVector2 world)
        {
            FPVector2 delta = world - Center;
            return new FPVector2(FPVector2.Dot(delta, Right), FPVector2.Dot(delta, Up));
        }

        public FPVector2 GetSupport(FPVector2 direction)
        {
            FP alongRight = FPVector2.Dot(direction, Right);
            FP alongUp = FPVector2.Dot(direction, Up);
            FP x = alongRight >= FP.Zero ? HalfExtents.x : -HalfExtents.x;
            FP y = alongUp >= FP.Zero ? HalfExtents.y : -HalfExtents.y;
            return ToWorld(x, y);
        }

        public FP RadiusAlong(FPVector2 axis)
        {
            return FP.Abs(FPVector2.Dot(axis, Right)) * HalfExtents.x
                + FP.Abs(FPVector2.Dot(axis, Up)) * HalfExtents.y;
        }
    }

    /// <summary>
    /// Contact manifold. Normal always points from body A toward body B.
    /// At most two contact points are generated (box-box clipping).
    /// </summary>
    internal struct CYGPhysics2DManifold
    {
        public FPVector2 Normal;
        public FP Penetration;
        public int PointCount;
        public FPVector2 Point0;
        public FPVector2 Point1;
        public FP Penetration0;
        public FP Penetration1;
    }

    internal static class CYGPhysics2DNarrowphase
    {
        /// <summary>Tests two bodies and emits a manifold with normal A to B.</summary>
        public static bool Collide(
            in CYGPhysics2DBody bodyA,
            in CYGPhysics2DBody bodyB,
            out CYGPhysics2DManifold manifold)
        {
            if (bodyA.Shape == CYGPhysics2DShapeType.Circle)
            {
                if (bodyB.Shape == CYGPhysics2DShapeType.Circle)
                {
                    return CollideCircleCircle(bodyA, bodyB, out manifold);
                }

                return CollideCircleBox(bodyA, bodyB, out manifold);
            }

            if (bodyB.Shape == CYGPhysics2DShapeType.Circle)
            {
                CYGPhysics2DManifold flipped;
                if (!CollideCircleBox(bodyB, bodyA, out flipped))
                {
                    manifold = default(CYGPhysics2DManifold);
                    return false;
                }

                manifold = flipped;
                manifold.Normal = -flipped.Normal;
                return true;
            }

            return CollideBoxBox(bodyA, bodyB, out manifold);
        }

        private static bool CollideCircleCircle(
            in CYGPhysics2DBody a,
            in CYGPhysics2DBody b,
            out CYGPhysics2DManifold manifold)
        {
            manifold = default(CYGPhysics2DManifold);

            FPVector2 delta = b.Position - a.Position;
            FP radiusSum = a.Radius + b.Radius;
            FP distanceSq = delta.SqrMagnitude;
            if (distanceSq > radiusSum * radiusSum)
            {
                return false;
            }

            FP distance = FP.Sqrt(distanceSq);
            FPVector2 normal = distance > FP.Zero ? delta / distance : FPVector2.Right;
            FP penetration = FP.Max(radiusSum - distance, FP.Zero);

            manifold.Normal = normal;
            manifold.Penetration = penetration;
            manifold.PointCount = 1;
            manifold.Point0 = a.Position + normal * (a.Radius - penetration * FP.Half);
            manifold.Penetration0 = penetration;
            return true;
        }

        /// <summary>Circle A versus oriented box B. Normal points circle to box.</summary>
        private static bool CollideCircleBox(
            in CYGPhysics2DBody circle,
            in CYGPhysics2DBody box,
            out CYGPhysics2DManifold manifold)
        {
            manifold = default(CYGPhysics2DManifold);
            CYGPhysics2DOBB obb = CYGPhysics2DOBB.Create(box);
            FPVector2 local = obb.ToLocal(circle.Position);
            FPVector2 half = obb.HalfExtents;

            bool inside = local.x >= -half.x && local.x <= half.x
                && local.y >= -half.y && local.y <= half.y;

            if (inside)
            {
                FP left = local.x + half.x;
                FP right = half.x - local.x;
                FP bottom = local.y + half.y;
                FP top = half.y - local.y;

                FP best = left;
                FPVector2 outward = new FPVector2(-FP.One, FP.Zero);

                if (right < best)
                {
                    best = right;
                    outward = new FPVector2(FP.One, FP.Zero);
                }

                if (bottom < best)
                {
                    best = bottom;
                    outward = new FPVector2(FP.Zero, -FP.One);
                }

                if (top < best)
                {
                    best = top;
                    outward = new FPVector2(FP.Zero, FP.One);
                }

                FPVector2 contactLocal = new FPVector2(local.x + outward.x * best, local.y + outward.y * best);
                manifold.Normal = obb.DirectionToWorld(-outward);
                manifold.Penetration = best + circle.Radius;
                manifold.PointCount = 1;
                manifold.Point0 = obb.ToWorld(contactLocal);
                manifold.Penetration0 = manifold.Penetration;
                return true;
            }

            FPVector2 closest = new FPVector2(
                FP.Clamp(local.x, -half.x, half.x),
                FP.Clamp(local.y, -half.y, half.y));
            FPVector2 deltaLocal = closest - local;
            FP distanceSq = deltaLocal.SqrMagnitude;
            FP radiusSq = circle.Radius * circle.Radius;
            if (distanceSq > radiusSq)
            {
                return false;
            }

            FP distance = FP.Sqrt(distanceSq);
            FPVector2 normalLocal = distance > FP.Zero ? deltaLocal / distance : new FPVector2(FP.One, FP.Zero);
            FP penetration = FP.Max(circle.Radius - distance, FP.Zero);

            manifold.Normal = obb.DirectionToWorld(normalLocal).Normalized;
            manifold.Penetration = penetration;
            manifold.PointCount = 1;
            manifold.Point0 = obb.ToWorld(closest);
            manifold.Penetration0 = penetration;
            return true;
        }

        private static bool CollideBoxBox(
            in CYGPhysics2DBody a,
            in CYGPhysics2DBody b,
            out CYGPhysics2DManifold manifold)
        {
            manifold = default(CYGPhysics2DManifold);
            CYGPhysics2DOBB boxA = CYGPhysics2DOBB.Create(a);
            CYGPhysics2DOBB boxB = CYGPhysics2DOBB.Create(b);
            FPVector2 delta = boxB.Center - boxA.Center;

            int found = 0;
            FP bestOverlap = FP.Zero;
            FPVector2 bestAxis = FPVector2.Right;
            bool bestFromA = true;

            if (!TestAxis(boxA, boxB, boxA.Right, delta, true, ref found, ref bestOverlap, ref bestAxis, ref bestFromA))
            {
                return false;
            }

            if (!TestAxis(boxA, boxB, boxA.Up, delta, true, ref found, ref bestOverlap, ref bestAxis, ref bestFromA))
            {
                return false;
            }

            if (!TestAxis(boxA, boxB, boxB.Right, delta, false, ref found, ref bestOverlap, ref bestAxis, ref bestFromA))
            {
                return false;
            }

            if (!TestAxis(boxA, boxB, boxB.Up, delta, false, ref found, ref bestOverlap, ref bestAxis, ref bestFromA))
            {
                return false;
            }

            if (found == 0)
            {
                return false;
            }

            FPVector2 normal = bestAxis;
            CYGPhysics2DOBB reference;
            CYGPhysics2DOBB incident;
            FPVector2 referenceNormal;

            if (bestFromA)
            {
                reference = boxA;
                incident = boxB;
                referenceNormal = normal;
            }
            else
            {
                reference = boxB;
                incident = boxA;
                referenceNormal = -normal;
            }

            FP referenceExtent = reference.RadiusAlong(referenceNormal);
            FPVector2 referenceFaceCenter = reference.Center + referenceNormal * referenceExtent;
            FPVector2 tangent = FPVector2.Perpendicular(referenceNormal);
            FP sideExtent = reference.RadiusAlong(tangent);

            FPVector2 edge0;
            FPVector2 edge1;
            SelectIncidentFace(incident, referenceNormal, out edge0, out edge1);

            FP d0 = FPVector2.Dot(edge0 - referenceFaceCenter, tangent);
            FP d1 = FPVector2.Dot(edge1 - referenceFaceCenter, tangent);
            FP min = -sideExtent;
            FP max = sideExtent;
            FP t0 = FP.Zero;
            FP t1 = FP.One;
            bool hasSegment = true;

            FP denominator = d1 - d0;
            if (denominator == FP.Zero)
            {
                hasSegment = d0 >= min && d0 <= max;
            }
            else
            {
                FP enterT = (min - d0) / denominator;
                FP exitT = (max - d0) / denominator;
                FP low = FP.Max(FP.Min(enterT, exitT), FP.Zero);
                FP high = FP.Min(FP.Max(enterT, exitT), FP.One);
                if (low > high)
                {
                    hasSegment = false;
                }
                else
                {
                    t0 = low;
                    t1 = high;
                }
            }

            FPVector2 point0 = FPVector2.Zero;
            FPVector2 point1 = FPVector2.Zero;
            FP penetration0 = FP.Zero;
            FP penetration1 = FP.Zero;
            int pointCount = 0;

            if (hasSegment)
            {
                FPVector2 edge = edge1 - edge0;
                point0 = edge0 + edge * t0;
                point1 = edge0 + edge * t1;

                FP separation0 = FPVector2.Dot(point0 - referenceFaceCenter, referenceNormal);
                if (separation0 <= FP.Zero)
                {
                    penetration0 = -separation0;
                    pointCount = 1;
                }

                if (t1 > t0)
                {
                    FP separation1 = FPVector2.Dot(point1 - referenceFaceCenter, referenceNormal);
                    if (separation1 <= FP.Zero)
                    {
                        if (pointCount == 0)
                        {
                            point0 = point1;
                            penetration0 = -separation1;
                            pointCount = 1;
                        }
                        else
                        {
                            penetration1 = -separation1;
                            pointCount = 2;
                        }
                    }
                }
            }

            if (pointCount == 0)
            {
                point0 = incident.GetSupport(-referenceNormal);
                penetration0 = FP.Max(bestOverlap, FP.Zero);
                pointCount = 1;
            }

            manifold.Normal = normal;
            manifold.PointCount = pointCount;
            manifold.Point0 = point0;
            manifold.Point1 = point1;
            manifold.Penetration0 = penetration0;
            manifold.Penetration1 = penetration1;
            manifold.Penetration = pointCount == 2 ? FP.Max(penetration0, penetration1) : penetration0;
            return true;
        }

        private static bool TestAxis(
            in CYGPhysics2DOBB boxA,
            in CYGPhysics2DOBB boxB,
            FPVector2 axis,
            FPVector2 delta,
            bool fromA,
            ref int found,
            ref FP bestOverlap,
            ref FPVector2 bestAxis,
            ref bool bestFromA)
        {
            FP radiusA = boxA.RadiusAlong(axis);
            FP radiusB = boxB.RadiusAlong(axis);
            FP distance = FPVector2.Dot(delta, axis);
            FP overlap = radiusA + radiusB - FP.Abs(distance);
            if (overlap < FP.Zero)
            {
                return false;
            }

            if (found == 0 || overlap < bestOverlap)
            {
                found = 1;
                bestOverlap = overlap;
                bestAxis = distance < FP.Zero ? -axis : axis;
                bestFromA = fromA;
            }

            return true;
        }

        /// <summary>
        /// Selects the incident box face whose outward normal most opposes the
        /// reference normal, and returns its two world-space endpoints.
        /// </summary>
        private static void SelectIncidentFace(
            in CYGPhysics2DOBB incident,
            FPVector2 referenceNormal,
            out FPVector2 edge0,
            out FPVector2 edge1)
        {
            FP dotRight = FPVector2.Dot(incident.Right, referenceNormal);
            FP dotUp = FPVector2.Dot(incident.Up, referenceNormal);

            if (FP.Abs(dotRight) >= FP.Abs(dotUp))
            {
                FPVector2 faceCenter = dotRight > FP.Zero
                    ? incident.Center - incident.Right * incident.HalfExtents.x
                    : incident.Center + incident.Right * incident.HalfExtents.x;
                edge0 = faceCenter + incident.Up * incident.HalfExtents.y;
                edge1 = faceCenter - incident.Up * incident.HalfExtents.y;
            }
            else
            {
                FPVector2 faceCenter = dotUp > FP.Zero
                    ? incident.Center - incident.Up * incident.HalfExtents.y
                    : incident.Center + incident.Up * incident.HalfExtents.y;
                edge0 = faceCenter + incident.Right * incident.HalfExtents.x;
                edge1 = faceCenter - incident.Right * incident.HalfExtents.x;
            }
        }

        /// <summary>Ray versus a body. Direction must be normalized.</summary>
        public static bool RaycastBody(
            in CYGPhysics2DBody body,
            FPVector2 origin,
            FPVector2 direction,
            FP maxDistance,
            out FP distance,
            out FPVector2 normal)
        {
            if (body.Shape == CYGPhysics2DShapeType.Circle)
            {
                return RaycastCircle(body, origin, direction, maxDistance, out distance, out normal);
            }

            return RaycastBox(body, origin, direction, maxDistance, out distance, out normal);
        }

        private static bool RaycastCircle(
            in CYGPhysics2DBody body,
            FPVector2 origin,
            FPVector2 direction,
            FP maxDistance,
            out FP distance,
            out FPVector2 normal)
        {
            distance = FP.Zero;
            normal = FPVector2.Zero;

            FPVector2 toOrigin = origin - body.Position;
            FP b = FPVector2.Dot(toOrigin, direction);
            FP c = toOrigin.SqrMagnitude - body.Radius * body.Radius;

            if (c > FP.Zero && b > FP.Zero)
            {
                return false;
            }

            FP discriminant = b * b - c;
            if (discriminant < FP.Zero)
            {
                return false;
            }

            FP root = FP.Sqrt(discriminant);
            FP t = -b - root;
            if (t < FP.Zero)
            {
                if (c > FP.Zero)
                {
                    return false;
                }

                t = FP.Zero;
            }

            if (t > maxDistance)
            {
                return false;
            }

            FPVector2 point = origin + direction * t;
            FPVector2 outward = point - body.Position;
            normal = outward.SqrMagnitude > FP.Zero ? outward.Normalized : -direction;
            distance = t;
            return true;
        }

        private static bool RaycastBox(
            in CYGPhysics2DBody body,
            FPVector2 origin,
            FPVector2 direction,
            FP maxDistance,
            out FP distance,
            out FPVector2 normal)
        {
            distance = FP.Zero;
            normal = FPVector2.Zero;

            CYGPhysics2DOBB obb = CYGPhysics2DOBB.Create(body);
            FPVector2 localOrigin = obb.ToLocal(origin);
            FPVector2 localDirection = new FPVector2(
                FPVector2.Dot(direction, obb.Right),
                FPVector2.Dot(direction, obb.Up));

            FP enter = FP.Zero;
            FP exit = maxDistance;
            FPVector2 localNormal = FPVector2.Zero;

            if (!RaycastSlab(
                    localOrigin.x, localDirection.x, obb.HalfExtents.x,
                    new FPVector2(-FP.One, FP.Zero), new FPVector2(FP.One, FP.Zero),
                    ref enter, ref exit, ref localNormal))
            {
                return false;
            }

            if (!RaycastSlab(
                    localOrigin.y, localDirection.y, obb.HalfExtents.y,
                    new FPVector2(FP.Zero, -FP.One), new FPVector2(FP.Zero, FP.One),
                    ref enter, ref exit, ref localNormal))
            {
                return false;
            }

            if (enter > exit || enter > maxDistance)
            {
                return false;
            }

            if (localNormal == FPVector2.Zero)
            {
                normal = -direction;
            }
            else
            {
                normal = obb.DirectionToWorld(localNormal).Normalized;
            }

            distance = enter;
            return true;
        }

        private static bool RaycastSlab(
            FP origin,
            FP direction,
            FP halfExtent,
            FPVector2 negativeNormal,
            FPVector2 positiveNormal,
            ref FP enter,
            ref FP exit,
            ref FPVector2 normal)
        {
            if (direction == FP.Zero)
            {
                return origin >= -halfExtent && origin <= halfExtent;
            }

            FP near = (-halfExtent - origin) / direction;
            FP far = (halfExtent - origin) / direction;
            FPVector2 nearNormal = negativeNormal;

            if (near > far)
            {
                FP swap = near;
                near = far;
                far = swap;
                nearNormal = positiveNormal;
            }

            if (near > enter)
            {
                enter = near;
                normal = nearNormal;
            }

            if (far < exit)
            {
                exit = far;
            }

            return enter <= exit;
        }

        /// <summary>Exact circle versus shape overlap test.</summary>
        public static bool OverlapCircleBody(in CYGPhysics2DBody body, FPVector2 center, FP radius)
        {
            if (body.Shape == CYGPhysics2DShapeType.Circle)
            {
                FP combined = radius + body.Radius;
                return FPVector2.DistanceSquared(center, body.Position) <= combined * combined;
            }

            CYGPhysics2DOBB obb = CYGPhysics2DOBB.Create(body);
            FPVector2 local = obb.ToLocal(center);
            FPVector2 closest = new FPVector2(
                FP.Clamp(local.x, -obb.HalfExtents.x, obb.HalfExtents.x),
                FP.Clamp(local.y, -obb.HalfExtents.y, obb.HalfExtents.y));
            FP distanceSq = (closest - local).SqrMagnitude;
            return distanceSq <= radius * radius;
        }

        /// <summary>Exact axis-aligned box versus shape overlap test.</summary>
        public static bool OverlapAABBBody(in CYGPhysics2DBody body, FPVector2 min, FPVector2 max)
        {
            if (body.Shape == CYGPhysics2DShapeType.Circle)
            {
                FPVector2 closest = new FPVector2(
                    FP.Clamp(body.Position.x, min.x, max.x),
                    FP.Clamp(body.Position.y, min.y, max.y));
                return FPVector2.DistanceSquared(body.Position, closest) <= body.Radius * body.Radius;
            }

            CYGPhysics2DOBB obb = CYGPhysics2DOBB.Create(body);
            FPVector2 queryCenter = (min + max) * FP.Half;
            FPVector2 queryExtents = (max - min) * FP.Half;
            FPVector2 delta = obb.Center - queryCenter;

            FPVector2[] axes = new FPVector2[4];
            axes[0] = FPVector2.Right;
            axes[1] = FPVector2.Up;
            axes[2] = obb.Right;
            axes[3] = obb.Up;

            for (int i = 0; i < axes.Length; i++)
            {
                FPVector2 axis = axes[i];
                FP queryRadius = FP.Abs(axis.x) * queryExtents.x + FP.Abs(axis.y) * queryExtents.y;
                FP shapeRadius = obb.RadiusAlong(axis);
                if (FP.Abs(FPVector2.Dot(delta, axis)) > queryRadius + shapeRadius)
                {
                    return false;
                }
            }

            return true;
        }
    }
}
