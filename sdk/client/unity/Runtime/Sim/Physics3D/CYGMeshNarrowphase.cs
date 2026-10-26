//-----------------------------------------------------------------------------
// CYGMeshNarrowphase.cs
// Dynamic primitive vs baked static triangle mesh contact generation.
// Zero UnityEngine dependencies. All math is Q32.32 fixed-point.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Narrowphase for a dynamic box/sphere/capsule against an immutable
    /// world-space triangle mesh. The mesh body is always the static participant;
    /// dynamic-vs-dynamic mesh bodies are rejected.
    /// </summary>
    internal static class CYGMeshNarrowphase
    {
        private const int CapsuleSamples = 8;
        private static readonly FP Epsilon = FP.FromFraction(1, 1000000);
        private static readonly FP ProjectionTolerance = FP.FromFraction(1, 1000);

        public static bool TryGetManifold(
            CYGPhysicsBody a,
            CYGPhysicsBody b,
            out CYGManifold manifold)
        {
            manifold = default(CYGManifold);

            CYGShapeType typeA = a.Collider.Shape.Type;
            CYGShapeType typeB = b.Collider.Shape.Type;
            bool meshA = typeA == CYGShapeType.Mesh;
            bool meshB = typeB == CYGShapeType.Mesh;
            if (!meshA && !meshB)
                return false;

            CYGPhysicsBody meshBody = meshA ? a : b;
            CYGPhysicsBody other = meshA ? b : a;
            if (meshA && meshB)
                return false;
            if (other.Type == CYGBodyType.Static)
                return false;
            if (meshBody.Type == CYGBodyType.Dynamic)
                return false;

            CYGStaticMesh mesh = CYGMeshRegistry.Get(meshBody.Collider.Shape.MeshId);
            if (mesh == null)
                return false;

            FPVector3 otherCenter = other.Collider.GetWorldCenter(other.Position, other.Rotation);
            CYGShape otherShape = other.Collider.Shape;
            FPAABB otherBounds = ShapeBounds(otherCenter, otherShape, other.Rotation);
            if (!mesh.Bounds.Intersects(otherBounds))
                return false;

            CYGMeshContactAccumulator contacts = default(CYGMeshContactAccumulator);
            switch (otherShape.Type)
            {
                case CYGShapeType.Sphere:
                    AccumulateSphere(mesh, otherCenter, otherShape.Radius, ref contacts);
                    break;
                case CYGShapeType.Capsule:
                    AccumulateCapsule(mesh, otherCenter, otherShape, other.Rotation, ref contacts);
                    break;
                case CYGShapeType.Box:
                    AccumulateBox(mesh, otherCenter, otherShape.HalfExtents, other.Rotation, ref contacts);
                    break;
                default:
                    return false;
            }

            if (contacts.Count == 0 || contacts.Normal == FPVector3.Zero)
                return false;

            manifold = default(CYGManifold);
            manifold.BodyA = a.Id;
            manifold.BodyB = b.Id;
            // Accumulated normals point from the mesh towards the dynamic body.
            manifold.Normal = meshA ? contacts.Normal : -contacts.Normal;
            for (int i = 0; i < contacts.Count; i++)
            {
                manifold.AddPoint(contacts.GetPoint(i), contacts.GetDepth(i));
            }
            return manifold.PointCount > 0;
        }

        private static void AccumulateSphere(
            CYGStaticMesh mesh,
            FPVector3 center,
            FP radius,
            ref CYGMeshContactAccumulator contacts)
        {
            FPVector3 extent = new FPVector3(radius, radius, radius);
            FPAABB query = new FPAABB(center - extent, center + extent);
            FP radiusSquared = radius * radius;

            for (int t = 0; t < mesh.TriangleCount; t++)
            {
                if (!mesh.GetTriangleBounds(t).Intersects(query))
                    continue;

                mesh.GetTriangle(t, out FPVector3 a, out FPVector3 b, out FPVector3 c);
                FPVector3 closest = CYGTriangleMath.ClosestPointOnTriangle(center, a, b, c);
                FPVector3 delta = center - closest;
                FP distanceSquared = delta.SqrMagnitude;
                if (distanceSquared >= radiusSquared)
                    continue;

                FP distance = FP.Sqrt(distanceSquared);
                FPVector3 normal = distance > Epsilon
                    ? delta / distance
                    : TriangleNormal(a, b, c, center);
                if (normal == FPVector3.Zero)
                    continue;

                contacts.Add(closest, radius - distance, normal);
            }
        }

        private static FPAABB ShapeBounds(FPVector3 center, CYGShape shape, FPQuaternion rotation)
        {
            if (shape.Type == CYGShapeType.Box)
                return CYGOrientedBox.GetAABB(center, shape.HalfExtents, rotation);
            if (shape.Type == CYGShapeType.Capsule)
            {
                FPVector3 axis = CYGOrientedBox.GetAxis(rotation, 1);
                FP half = shape.GetCapsuleSegmentHalfLength();
                FPVector3 extent = new FPVector3(shape.Radius, shape.Radius, shape.Radius)
                    + new FPVector3(FP.Abs(axis.x), FP.Abs(axis.y), FP.Abs(axis.z)) * half;
                return new FPAABB(center - extent, center + extent);
            }
            return shape.GetAABB(center);
        }

        private static void AccumulateCapsule(
            CYGStaticMesh mesh,
            FPVector3 center,
            CYGShape shape,
            FPQuaternion rotation,
            ref CYGMeshContactAccumulator contacts)
        {
            FP half = shape.GetCapsuleSegmentHalfLength();
            FPVector3 axis = CYGOrientedBox.GetAxis(rotation, 1);
            FPVector3 bottom = center - axis * half;
            FPVector3 step = axis * ((half * FP.FromInt(2)) / FP.FromInt(CapsuleSamples - 1));

            for (int i = 0; i < CapsuleSamples; i++)
            {
                AccumulateSphere(mesh, bottom + step * FP.FromInt(i), shape.Radius, ref contacts);
            }
        }

        private static void AccumulateBox(
            CYGStaticMesh mesh,
            FPVector3 center,
            FPVector3 halfExtents,
            FPQuaternion rotation,
            ref CYGMeshContactAccumulator contacts)
        {
            FPAABB boxBounds = CYGOrientedBox.GetAABB(center, halfExtents, rotation);
            CYGOrientedBox.GetAxes(rotation, out FPVector3 axisX, out FPVector3 axisY, out FPVector3 axisZ);

            for (int t = 0; t < mesh.TriangleCount; t++)
            {
                if (!mesh.GetTriangleBounds(t).Intersects(boxBounds))
                    continue;

                mesh.GetTriangle(t, out FPVector3 a, out FPVector3 b, out FPVector3 c);
                FPVector3 normal = TriangleNormal(a, b, c, center);
                if (normal == FPVector3.Zero)
                    continue;

                for (int cornerIndex = 0; cornerIndex < 8; cornerIndex++)
                {
                    FPVector3 corner = center
                        + axisX * (((cornerIndex & 1) != 0) ? halfExtents.x : -halfExtents.x)
                        + axisY * (((cornerIndex & 2) != 0) ? halfExtents.y : -halfExtents.y)
                        + axisZ * (((cornerIndex & 4) != 0) ? halfExtents.z : -halfExtents.z);

                    FP side = FPVector3.Dot(corner - a, normal);
                    if (side >= -Epsilon)
                        continue;

                    FPVector3 closest = CYGTriangleMath.ClosestPointOnTriangle(corner, a, b, c);
                    FP distance = (corner - closest).Magnitude;
                    // Accept only projections inside the triangle; edge hits would
                    // produce phantom contacts far from the box corner.
                    if (distance > -side + ProjectionTolerance)
                        continue;

                    contacts.Add(closest, -side, normal);
                }
            }
        }

        private static FPVector3 TriangleNormal(
            FPVector3 a,
            FPVector3 b,
            FPVector3 c,
            FPVector3 toward)
        {
            FPVector3 cross = FPVector3.Cross(b - a, c - a);
            FP length = cross.Magnitude;
            if (length <= Epsilon)
                return FPVector3.Zero;
            FPVector3 normal = cross / length;
            return FPVector3.Dot(normal, toward - a) >= FP.Zero ? normal : -normal;
        }
    }
}
