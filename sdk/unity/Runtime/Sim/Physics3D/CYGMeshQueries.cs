using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Fixed-point ray and sphere queries against <see cref="CYGStaticMesh"/>.
    /// All routines are deterministic: they use only FP arithmetic and a fixed
    /// iteration cap, never UnityEngine physics or floating-point helpers.
    /// </summary>
    internal static class CYGMeshQueries
    {
        private const int CastIterations = 24;

        public static bool Raycast(
            CYGStaticMesh mesh,
            FPRay ray,
            FP maxDistance,
            out FPRaycastHit hit)
        {
            hit = FPRaycastHit.None;
            if (maxDistance <= FP.Zero)
            {
                return false;
            }

            FP directionLength = ray.Direction.Magnitude;
            if (directionLength <= CYGTriangleMath.Epsilon)
            {
                return false;
            }

            FPVector3 direction = ray.Direction / directionLength;
            FPRay probe = new FPRay(ray.Origin, direction);
            if (!FPRaycast.RaycastAABB(probe, mesh.Bounds, maxDistance, out _))
            {
                return false;
            }

            bool found = false;
            FP best = maxDistance;

            for (int triangle = 0; triangle < mesh.TriangleCount; triangle++)
            {
                if (!mesh.RaycastTriangleBounds(probe, triangle, best))
                {
                    continue;
                }

                mesh.GetTriangle(triangle, out FPVector3 a, out FPVector3 b, out FPVector3 c);
                if (!CYGTriangleMath.RayTriangle(ray.Origin, direction, a, b, c, best,
                        out FP distance, out FPVector3 point, out FPVector3 normal))
                {
                    continue;
                }

                best = distance;
                found = true;
                hit = new FPRaycastHit
                {
                    Hit = true,
                    Distance = distance,
                    Point = point,
                    Normal = normal,
                    Fraction = CYGTriangleMath.Fraction(distance, maxDistance),
                };
            }

            return found;
        }

        public static bool SphereCast(
            CYGStaticMesh mesh,
            FPSphere sphere,
            FPVector3 direction,
            FP maxDistance,
            out FPRaycastHit hit)
        {
            hit = FPRaycastHit.None;
            if (maxDistance <= FP.Zero)
            {
                return false;
            }

            FP directionLength = direction.Magnitude;
            if (directionLength <= CYGTriangleMath.Epsilon)
            {
                return OverlapHit(mesh, sphere, out hit);
            }

            FPVector3 normalDirection = direction / directionLength;
            if (!SweepIntersects(mesh, sphere.Center, sphere.Radius, normalDirection, maxDistance))
            {
                return false;
            }

            bool found = false;
            FP best = maxDistance;

            for (int triangle = 0; triangle < mesh.TriangleCount; triangle++)
            {
                mesh.GetTriangle(triangle, out FPVector3 a, out FPVector3 b, out FPVector3 c);
                if (!TrySphereTriangle(
                        sphere.Center, sphere.Radius, normalDirection, maxDistance,
                        a, b, c, out FP distance, out FPVector3 point, out FPVector3 normal))
                {
                    continue;
                }

                if (distance < best)
                {
                    best = distance;
                    found = true;
                    hit = new FPRaycastHit
                    {
                        Hit = true,
                        Distance = distance,
                        Point = point,
                        Normal = normal,
                        Fraction = CYGTriangleMath.Fraction(distance, maxDistance),
                    };
                }
            }

            return found;
        }

        /// <summary>
        /// Conservative advancement of a sphere against one triangle. The
        /// direction must be normalized. The iteration cap keeps the result
        /// deterministic even for grazing contacts.
        /// </summary>
        private static bool TrySphereTriangle(
            FPVector3 origin,
            FP radius,
            FPVector3 direction,
            FP maxDistance,
            FPVector3 a,
            FPVector3 b,
            FPVector3 c,
            out FP distance,
            out FPVector3 point,
            out FPVector3 normal)
        {
            distance = FP.Zero;
            point = origin;
            normal = FPVector3.Up;

            FP radiusSquared = radius * radius;
            FP travelled = FP.Zero;

            for (int iteration = 0; iteration < CastIterations; iteration++)
            {
                FPVector3 center = origin + direction * travelled;
                FPVector3 closest = CYGTriangleMath.ClosestPointOnTriangle(center, a, b, c);
                FPVector3 delta = center - closest;
                FP squared = delta.SqrMagnitude;

                if (squared <= radiusSquared)
                {
                    distance = travelled;
                    point = closest;
                    normal = delta.SqrMagnitude > CYGTriangleMath.Epsilon * CYGTriangleMath.Epsilon
                        ? delta.Normalized
                        : CYGTriangleMath.FaceNormal(a, b, c, direction);
                    return true;
                }

                FP separation = FP.Sqrt(squared) - radius;
                if (separation <= CYGTriangleMath.Epsilon)
                {
                    distance = travelled;
                    point = closest;
                    normal = CYGTriangleMath.FaceNormal(a, b, c, direction);
                    return true;
                }

                travelled += separation;
                if (travelled > maxDistance)
                {
                    return false;
                }
            }

            return false;
        }

        private static bool OverlapHit(CYGStaticMesh mesh, FPSphere sphere, out FPRaycastHit hit)
        {
            if (!CYGMeshCasts.Overlaps(mesh, sphere))
            {
                hit = FPRaycastHit.None;
                return false;
            }

            hit = new FPRaycastHit
            {
                Hit = true,
                Distance = FP.Zero,
                Point = sphere.Center,
                Normal = FPVector3.Up,
                Fraction = FP.Zero,
            };
            return true;
        }

        private static bool SweepIntersects(
            CYGStaticMesh mesh,
            FPVector3 center,
            FP radius,
            FPVector3 direction,
            FP maxDistance)
        {
            FPVector3 extent = new FPVector3(radius, radius, radius);
            FPAABB sweep = new FPAABB(center - extent, center + extent);
            sweep.Encapsulate(center + direction * maxDistance + extent);
            sweep.Encapsulate(center + direction * maxDistance - extent);
            return mesh.Bounds.Intersects(sweep);
        }
    }
}
