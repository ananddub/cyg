//-----------------------------------------------------------------------------
// CYGPhysicsNarrowphase.cs
// Deterministic narrowphase and contact manifolds for the CYG 3D rigid body
// runtime, including oriented boxes, spheres, capsules, static meshes and
// rotational contact response.
// Zero UnityEngine dependencies. All math is Q32.32 fixed-point.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// One point of a contact manifold plus the impulses accumulated during the
    /// current step. Impulses are reset every step (no cross-step warm starting).
    /// </summary>
    [Serializable]
    public struct CYGContactPoint
    {
        public FPVector3 Point;
        public FP Depth;
        public FP NormalImpulse;
        public FP TangentImpulse1;
        public FP TangentImpulse2;
        public FP RestitutionBias;
    }

    /// <summary>
    /// Deterministic contact manifold. <see cref="Normal"/> points from
    /// <see cref="BodyA"/> towards <see cref="BodyB"/> and <see cref="BodyA"/> has
    /// the smaller body id. At most eight contact points are stored so a
    /// clipped rotated-box face keeps its full (possibly octagonal) patch
    /// instead of a biased subset.
    /// </summary>
    [Serializable]
    public struct CYGManifold
    {
        public int BodyA;
        public int BodyB;
        public FPVector3 Normal;
        public FPVector3 Tangent1;
        public FPVector3 Tangent2;
        public FP MaxDepth;
        public int PointCount;

        public CYGContactPoint Point0;
        public CYGContactPoint Point1;
        public CYGContactPoint Point2;
        public CYGContactPoint Point3;
        public CYGContactPoint Point4;
        public CYGContactPoint Point5;
        public CYGContactPoint Point6;
        public CYGContactPoint Point7;

        public long PairKey
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => ((long)BodyA << 32) | (uint)BodyB;
        }

        public CYGContactPoint GetPoint(int index)
        {
            switch (index)
            {
                case 0: return Point0;
                case 1: return Point1;
                case 2: return Point2;
                case 3: return Point3;
                case 4: return Point4;
                case 5: return Point5;
                case 6: return Point6;
                default: return Point7;
            }
        }

        public void SetPoint(int index, CYGContactPoint value)
        {
            switch (index)
            {
                case 0: Point0 = value; break;
                case 1: Point1 = value; break;
                case 2: Point2 = value; break;
                case 3: Point3 = value; break;
                case 4: Point4 = value; break;
                case 5: Point5 = value; break;
                case 6: Point6 = value; break;
                default: Point7 = value; break;
            }
        }

        public void AddPoint(FPVector3 point, FP depth)
        {
            if (PointCount >= 8)
                return;

            CYGContactPoint contactPoint = default(CYGContactPoint);
            contactPoint.Point = point;
            contactPoint.Depth = depth;
            SetPoint(PointCount, contactPoint);
            PointCount++;
            if (depth > MaxDepth)
                MaxDepth = depth;
        }
    }

    /// <summary>
    /// Contact generation for oriented boxes, spheres, capsules and static
    /// triangle meshes. Normals always point from body A to body B and pairs are
    /// passed with BodyA.Id &lt; BodyB.Id.
    /// </summary>
    internal static class CYGPhysicsNarrowphase
    {
        private static readonly FP Epsilon = FP.FromFraction(1, 1000000);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private static FPVector3 ShapeCenter(CYGPhysicsBody body)
        {
            return body.Collider.GetWorldCenter(body.Position, body.Rotation);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private static FPVector3 BoxHalfExtents(CYGPhysicsBody body)
        {
            return body.Collider.Shape.HalfExtents;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private static void CapsuleSegment(CYGPhysicsBody body, FPVector3 center, out FPVector3 pointA, out FPVector3 pointB)
        {
            FP half = body.Collider.Shape.GetCapsuleSegmentHalfLength();
            FPVector3 axis = CYGOrientedBox.GetAxis(body.Rotation, 1);
            pointA = center - axis * half;
            pointB = center + axis * half;
        }

        private static bool OrientedCapsuleBox(
            CYGPhysicsBody capsule,
            CYGPhysicsBody box,
            FPVector3 capsuleCenter,
            FPVector3 boxCenter,
            bool capsuleIsA,
            out CYGManifold manifold)
        {
            manifold = default(CYGManifold);
            if (!CYGOrientedBox.CapsuleBox(
                    capsule, capsuleCenter, boxCenter, BoxHalfExtents(box), box.Rotation,
                    out CYGOrientedContacts contacts))
                return false;

            manifold.BodyA = capsuleIsA ? capsule.Id : box.Id;
            manifold.BodyB = capsuleIsA ? box.Id : capsule.Id;
            manifold.Normal = capsuleIsA ? -contacts.Normal : contacts.Normal;
            for (int i = 0; i < contacts.Count; i++)
                manifold.AddPoint(contacts.GetPoint(i), contacts.GetDepth(i));
            return manifold.PointCount > 0;
        }

        public static bool Overlaps(CYGPhysicsBody a, CYGPhysicsBody b)
        {
            CYGManifold manifold;
            return TryGetManifold(a, b, out manifold);
        }

        /// <summary>
        /// Generates a manifold for the pair. The normal points from A towards B.
        /// </summary>
        public static bool TryGetManifold(CYGPhysicsBody a, CYGPhysicsBody b, out CYGManifold manifold)
        {
            CYGShapeType typeA = a.Collider.Shape.Type;
            CYGShapeType typeB = b.Collider.Shape.Type;
            if (typeA == CYGShapeType.Mesh || typeB == CYGShapeType.Mesh)
                return CYGMeshNarrowphase.TryGetManifold(a, b, out manifold);

            FPVector3 centerA = ShapeCenter(a);
            FPVector3 centerB = ShapeCenter(b);

            if (typeA == CYGShapeType.Box && typeB == CYGShapeType.Box)
            {
                if (a.Rotation == FPQuaternion.Identity && b.Rotation == FPQuaternion.Identity)
                    return BoxBox(a, b, centerA, centerB, out manifold);
                return CYGOrientedBox.BoxBox(a, b, centerA, centerB, out manifold);
            }

            if (typeA == CYGShapeType.Sphere && typeB == CYGShapeType.Sphere)
                return SphereSphere(a, b, centerA, a.Collider.Shape.Radius, centerB, b.Collider.Shape.Radius, out manifold);

            if (typeA == CYGShapeType.Box && typeB == CYGShapeType.Sphere)
            {
                if (a.Rotation != FPQuaternion.Identity)
                {
                    if (!CYGOrientedBox.SphereBox(
                            centerB, b.Collider.Shape.Radius, centerA, BoxHalfExtents(a), a.Rotation,
                            out FPVector3 normalBoxToSphere, out FPVector3 point, out FP depth))
                    {
                        manifold = default(CYGManifold);
                        return false;
                    }
                    manifold = CreateSingle(a, b, normalBoxToSphere, point, depth);
                    return true;
                }

                if (!SphereBox(centerB, b.Collider.Shape.Radius, centerA, BoxHalfExtents(a), out FPVector3 oldNormal, out FPVector3 oldPoint, out FP oldDepth))
                {
                    manifold = default(CYGManifold);
                    return false;
                }
                manifold = CreateSingle(a, b, oldNormal, oldPoint, oldDepth);
                return true;
            }

            if (typeA == CYGShapeType.Sphere && typeB == CYGShapeType.Box)
            {
                if (b.Rotation != FPQuaternion.Identity)
                {
                    if (!CYGOrientedBox.SphereBox(
                            centerA, a.Collider.Shape.Radius, centerB, BoxHalfExtents(b), b.Rotation,
                            out FPVector3 normalBoxToSphere, out FPVector3 point, out FP depth))
                    {
                        manifold = default(CYGManifold);
                        return false;
                    }
                    manifold = CreateSingle(a, b, -normalBoxToSphere, point, depth);
                    return true;
                }

                if (!SphereBox(centerA, a.Collider.Shape.Radius, centerB, BoxHalfExtents(b), out FPVector3 oldNormal, out FPVector3 oldPoint, out FP oldDepth))
                {
                    manifold = default(CYGManifold);
                    return false;
                }
                manifold = CreateSingle(a, b, -oldNormal, oldPoint, oldDepth);
                return true;
            }

            if (typeA == CYGShapeType.Capsule && typeB == CYGShapeType.Box)
            {
                if (a.Rotation == FPQuaternion.Identity && b.Rotation == FPQuaternion.Identity)
                {
                    if (!CapsuleBox(a, centerA, centerB, BoxHalfExtents(b), out FPVector3 normalCapsuleToBox, out FPVector3 point, out FP depth))
                    {
                        manifold = default(CYGManifold);
                        return false;
                    }
                    manifold = CreateSingle(a, b, normalCapsuleToBox, point, depth);
                    return true;
                }

                return OrientedCapsuleBox(a, b, centerA, centerB, true, out manifold);
            }

            if (typeA == CYGShapeType.Box && typeB == CYGShapeType.Capsule)
            {
                if (a.Rotation == FPQuaternion.Identity && b.Rotation == FPQuaternion.Identity)
                {
                    if (!CapsuleBox(b, centerB, centerA, BoxHalfExtents(a), out FPVector3 normalCapsuleToBox, out FPVector3 point, out FP depth))
                    {
                        manifold = default(CYGManifold);
                        return false;
                    }
                    manifold = CreateSingle(a, b, -normalCapsuleToBox, point, depth);
                    return true;
                }

                return OrientedCapsuleBox(b, a, centerB, centerA, false, out manifold);
            }

            if (typeA == CYGShapeType.Capsule && typeB == CYGShapeType.Sphere)
            {
                FPVector3 normalCapsuleToSphere;
                FPVector3 point;
                FP depth;
                if (!CapsuleSphere(a, centerA, b.Collider.Shape.Radius, centerB, out normalCapsuleToSphere, out point, out depth))
                {
                    manifold = default(CYGManifold);
                    return false;
                }
                manifold = CreateSingle(a, b, normalCapsuleToSphere, point, depth);
                return true;
            }

            if (typeA == CYGShapeType.Sphere && typeB == CYGShapeType.Capsule)
            {
                FPVector3 normalCapsuleToSphere;
                FPVector3 point;
                FP depth;
                if (!CapsuleSphere(b, centerB, a.Collider.Shape.Radius, centerA, out normalCapsuleToSphere, out point, out depth))
                {
                    manifold = default(CYGManifold);
                    return false;
                }
                manifold = CreateSingle(a, b, -normalCapsuleToSphere, point, depth);
                return true;
            }

            // Capsule/capsule is the only remaining combination.
            return CapsuleCapsule(a, b, centerA, centerB, out manifold);
        }

        private static CYGManifold CreateSingle(CYGPhysicsBody a, CYGPhysicsBody b, FPVector3 normal, FPVector3 point, FP depth)
        {
            CYGManifold manifold = default(CYGManifold);
            manifold.BodyA = a.Id;
            manifold.BodyB = b.Id;
            manifold.Normal = normal;
            manifold.AddPoint(point, depth);
            return manifold;
        }

        private static bool SphereSphere(
            CYGPhysicsBody a,
            CYGPhysicsBody b,
            FPVector3 centerA,
            FP radiusA,
            FPVector3 centerB,
            FP radiusB,
            out CYGManifold manifold)
        {
            manifold = default(CYGManifold);

            FPVector3 delta = centerB - centerA;
            FP radiusSum = radiusA + radiusB;
            FP distanceSquared = delta.SqrMagnitude;
            if (distanceSquared > radiusSum * radiusSum)
                return false;

            FP distance = FP.Sqrt(distanceSquared);
            FPVector3 normal = distance > Epsilon ? delta / distance : FPVector3.Up;
            FP depth = radiusSum - distance;
            FPVector3 point = centerA + normal * (radiusA - depth * FP.Half);

            manifold = CreateSingle(a, b, normal, point, depth);
            return true;
        }

        /// <summary>
        /// Sphere versus box. The returned normal points from the box towards the
        /// sphere. Handles sphere centers inside the box with a fixed face tie-break.
        /// </summary>
        private static bool SphereBox(
            FPVector3 sphereCenter,
            FP radius,
            FPVector3 boxCenter,
            FPVector3 halfExtents,
            out FPVector3 normalBoxToSphere,
            out FPVector3 point,
            out FP depth)
        {
            normalBoxToSphere = FPVector3.Up;
            point = sphereCenter;
            depth = FP.Zero;

            FPAABB box = new FPAABB(boxCenter - halfExtents, boxCenter + halfExtents);
            FPVector3 closest = box.ClosestPoint(sphereCenter);
            FPVector3 delta = sphereCenter - closest;
            FP distanceSquared = delta.SqrMagnitude;

            if (distanceSquared > radius * radius)
                return false;

            if (distanceSquared > Epsilon * Epsilon)
            {
                FP distance = FP.Sqrt(distanceSquared);
                normalBoxToSphere = delta / distance;
                depth = radius - distance;
                point = closest;
                return true;
            }

            // The sphere center is inside or exactly on the box surface. Pick the
            // nearest face with a fixed x/y/z and min/max tie-break order.
            FP dxMin = sphereCenter.x - box.Min.x;
            FP dxMax = box.Max.x - sphereCenter.x;
            FP dyMin = sphereCenter.y - box.Min.y;
            FP dyMax = box.Max.y - sphereCenter.y;
            FP dzMin = sphereCenter.z - box.Min.z;
            FP dzMax = box.Max.z - sphereCenter.z;

            FP best = dxMin;
            normalBoxToSphere = FPVector3.Left;
            point = new FPVector3(box.Min.x, sphereCenter.y, sphereCenter.z);

            if (dxMax < best)
            {
                best = dxMax;
                normalBoxToSphere = FPVector3.Right;
                point = new FPVector3(box.Max.x, sphereCenter.y, sphereCenter.z);
            }
            if (dyMin < best)
            {
                best = dyMin;
                normalBoxToSphere = FPVector3.Down;
                point = new FPVector3(sphereCenter.x, box.Min.y, sphereCenter.z);
            }
            if (dyMax < best)
            {
                best = dyMax;
                normalBoxToSphere = FPVector3.Up;
                point = new FPVector3(sphereCenter.x, box.Max.y, sphereCenter.z);
            }
            if (dzMin < best)
            {
                best = dzMin;
                normalBoxToSphere = FPVector3.Back;
                point = new FPVector3(sphereCenter.x, sphereCenter.y, box.Min.z);
            }
            if (dzMax < best)
            {
                best = dzMax;
                normalBoxToSphere = FPVector3.Forward;
                point = new FPVector3(sphereCenter.x, sphereCenter.y, box.Max.z);
            }

            depth = radius + best;
            return true;
        }

        private static bool BoxBox(
            CYGPhysicsBody a,
            CYGPhysicsBody b,
            FPVector3 centerA,
            FPVector3 centerB,
            out CYGManifold manifold)
        {
            manifold = default(CYGManifold);

            FPVector3 halfA = BoxHalfExtents(a);
            FPVector3 halfB = BoxHalfExtents(b);
            FPVector3 delta = centerB - centerA;

            FP overlapX = halfA.x + halfB.x - FP.Abs(delta.x);
            FP overlapY = halfA.y + halfB.y - FP.Abs(delta.y);
            FP overlapZ = halfA.z + halfB.z - FP.Abs(delta.z);

            if (overlapX < FP.Zero || overlapY < FP.Zero || overlapZ < FP.Zero)
                return false;

            FPVector3 normal;
            FP depth;
            int normalAxis;

            // Pick the least-overlapping axis by relative penetration. Raw AABB
            // overlap is unreliable for stacked boxes with coincident footprints:
            // a tiny vertical penetration makes overlapX the numeric minimum, which
            // would push the stack sideways instead of supporting it. Comparing
            // overlap over the combined half extents keeps the vertical face normal
            // for stacks while still resolving edge contacts correctly.
            FP relativeX = overlapX / (halfA.x + halfB.x);
            FP relativeY = overlapY / (halfA.y + halfB.y);
            FP relativeZ = overlapZ / (halfA.z + halfB.z);

            // Fixed tie-break: Y, then X, then Z.
            if (relativeY <= relativeX && relativeY <= relativeZ)
            {
                normalAxis = 1;
                depth = overlapY;
                normal = delta.y >= FP.Zero ? FPVector3.Up : FPVector3.Down;
            }
            else if (relativeX <= relativeZ)
            {
                normalAxis = 0;
                depth = overlapX;
                normal = delta.x >= FP.Zero ? FPVector3.Right : FPVector3.Left;
            }
            else
            {
                normalAxis = 2;
                depth = overlapZ;
                normal = delta.z >= FP.Zero ? FPVector3.Forward : FPVector3.Back;
            }

            FPAABB aabbA = new FPAABB(centerA - halfA, centerA + halfA);
            FPAABB aabbB = new FPAABB(centerB - halfB, centerB + halfB);
            FPVector3 overlapMin = FPVector3.Max(aabbA.Min, aabbB.Min);
            FPVector3 overlapMax = FPVector3.Min(aabbA.Max, aabbB.Max);

            manifold.BodyA = a.Id;
            manifold.BodyB = b.Id;
            manifold.Normal = normal;
            manifold.MaxDepth = depth;

            FP normalCoordinate = Component((overlapMin + overlapMax) * FP.Half, normalAxis);
            int tangentAxisU;
            int tangentAxisV;
            if (normalAxis == 0)
            {
                tangentAxisU = 1;
                tangentAxisV = 2;
            }
            else if (normalAxis == 1)
            {
                tangentAxisU = 0;
                tangentAxisV = 2;
            }
            else
            {
                tangentAxisU = 0;
                tangentAxisV = 1;
            }

            FP uMin = Component(overlapMin, tangentAxisU);
            FP uMax = Component(overlapMax, tangentAxisU);
            FP vMin = Component(overlapMin, tangentAxisV);
            FP vMax = Component(overlapMax, tangentAxisV);

            bool openU = uMax - uMin > Epsilon;
            bool openV = vMax - vMin > Epsilon;
            int countU = openU ? 2 : 1;
            int countV = openV ? 2 : 1;

            for (int iu = 0; iu < countU; iu++)
            {
                FP u = iu == 0 ? uMin : uMax;
                for (int iv = 0; iv < countV; iv++)
                {
                    FP v = iv == 0 ? vMin : vMax;
                    FPVector3 point = FPVector3.Zero;
                    SetComponent(ref point, normalAxis, normalCoordinate);
                    SetComponent(ref point, tangentAxisU, u);
                    SetComponent(ref point, tangentAxisV, v);
                    manifold.AddPoint(point, depth);
                }
            }

            return manifold.PointCount > 0;
        }

        private static bool CapsuleSphere(
            CYGPhysicsBody capsule,
            FPVector3 capsuleCenter,
            FP sphereRadius,
            FPVector3 sphereCenter,
            out FPVector3 normalCapsuleToSphere,
            out FPVector3 point,
            out FP depth)
        {
            normalCapsuleToSphere = FPVector3.Up;
            point = sphereCenter;
            depth = FP.Zero;

            FPVector3 segmentA;
            FPVector3 segmentB;
            CapsuleSegment(capsule, capsuleCenter, out segmentA, out segmentB);
            FPVector3 closest = ClosestPointOnSegment(segmentA, segmentB, sphereCenter);

            FPVector3 delta = sphereCenter - closest;
            FP radiusSum = capsule.Collider.Shape.Radius + sphereRadius;
            FP distanceSquared = delta.SqrMagnitude;
            if (distanceSquared > radiusSum * radiusSum)
                return false;

            FP distance = FP.Sqrt(distanceSquared);
            normalCapsuleToSphere = distance > Epsilon ? delta / distance : FPVector3.Up;
            depth = radiusSum - distance;
            point = closest + normalCapsuleToSphere * (capsule.Collider.Shape.Radius - depth * FP.Half);
            return true;
        }

        private static bool CapsuleCapsule(
            CYGPhysicsBody a,
            CYGPhysicsBody b,
            FPVector3 centerA,
            FPVector3 centerB,
            out CYGManifold manifold)
        {
            manifold = default(CYGManifold);

            FPVector3 a0;
            FPVector3 a1;
            FPVector3 b0;
            FPVector3 b1;
            CapsuleSegment(a, centerA, out a0, out a1);
            CapsuleSegment(b, centerB, out b0, out b1);

            FPVector3 closestA;
            FPVector3 closestB;
            FPCapsule.ClosestPointsBetweenSegments(a0, a1, b0, b1, out closestA, out closestB);

            FP radiusSum = a.Collider.Shape.Radius + b.Collider.Shape.Radius;
            FPVector3 delta = closestB - closestA;
            FP distanceSquared = delta.SqrMagnitude;
            if (distanceSquared > radiusSum * radiusSum)
                return false;

            FP distance = FP.Sqrt(distanceSquared);
            FPVector3 normal = distance > Epsilon ? delta / distance : FPVector3.Up;
            FP depth = radiusSum - distance;
            FPVector3 point = closestA + normal * (a.Collider.Shape.Radius - depth * FP.Half);

            manifold = CreateSingle(a, b, normal, point, depth);
            return true;
        }

        /// <summary>
        /// Vertical capsule versus AABB. The capsule segment is sampled at the box
        /// center's height clamped to the segment span, which is exact for a vertical
        /// capsule because both primitives are axis-aligned.
        /// </summary>
        private static bool CapsuleBox(
            CYGPhysicsBody capsule,
            FPVector3 capsuleCenter,
            FPVector3 boxCenter,
            FPVector3 boxHalfExtents,
            out FPVector3 normalCapsuleToBox,
            out FPVector3 point,
            out FP depth)
        {
            normalCapsuleToBox = FPVector3.Up;
            point = boxCenter;
            depth = FP.Zero;

            FPVector3 segmentA;
            FPVector3 segmentB;
            CapsuleSegment(capsule, capsuleCenter, out segmentA, out segmentB);

            FP sampleY = FP.Clamp(boxCenter.y, segmentA.y, segmentB.y);
            FPVector3 sphereCenter = new FPVector3(capsuleCenter.x, sampleY, capsuleCenter.z);

            FPVector3 normalBoxToSphere;
            if (!SphereBox(
                    sphereCenter,
                    capsule.Collider.Shape.Radius,
                    boxCenter,
                    boxHalfExtents,
                    out normalBoxToSphere,
                    out point,
                    out depth))
            {
                return false;
            }

            normalCapsuleToBox = -normalBoxToSphere;
            return true;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private static FP Component(FPVector3 v, int axis)
        {
            if (axis == 0) return v.x;
            if (axis == 1) return v.y;
            return v.z;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private static void SetComponent(ref FPVector3 v, int axis, FP value)
        {
            if (axis == 0) v.x = value;
            else if (axis == 1) v.y = value;
            else v.z = value;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private static FPVector3 ClosestPointOnSegment(FPVector3 a, FPVector3 b, FPVector3 point)
        {
            FPVector3 ab = b - a;
            FP lengthSquared = ab.SqrMagnitude;
            if (lengthSquared < Epsilon)
                return a;
            FP t = FPVector3.Dot(point - a, ab) / lengthSquared;
            t = FP.Clamp01(t);
            return a + ab * t;
        }
    }
}
