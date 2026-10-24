using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Fixed-point capsule casts and overlap tests against static meshes.
    /// Capsules are sampled along their segment with a fixed number of spheres,
    /// which keeps the cost bounded and the result deterministic.
    /// </summary>
    internal static class CYGMeshCasts
    {
        private const int CapsuleSamples = 5;

        public static bool CapsuleCast(
            CYGStaticMesh mesh,
            FPCapsule capsule,
            FPVector3 direction,
            FP maxDistance,
            out FPRaycastHit hit)
        {
            hit = FPRaycastHit.None;
            FP directionLength = direction.Magnitude;
            if (directionLength <= CYGTriangleMath.Epsilon)
            {
                return Overlaps(mesh, capsule);
            }

            FPVector3 normalDirection = direction / directionLength;
            bool found = false;
            FP best = maxDistance;

            for (int sample = 0; sample < CapsuleSamples; sample++)
            {
                FP t = FP.FromFraction(sample, CapsuleSamples - 1);
                FPVector3 center = capsule.PointA + (capsule.PointB - capsule.PointA) * t;
                FPSphere sphere = new FPSphere(center, capsule.Radius);
                if (!CYGMeshQueries.SphereCast(mesh, sphere, normalDirection, best, out FPRaycastHit sampleHit))
                {
                    continue;
                }

                if (sampleHit.Distance < best)
                {
                    best = sampleHit.Distance;
                    found = true;
                    hit = sampleHit;
                }
            }

            return found;
        }

        public static bool Overlaps(CYGStaticMesh mesh, FPSphere sphere)
        {
            if (!mesh.Bounds.Intersects(sphere))
            {
                return false;
            }

            FP radiusSquared = sphere.Radius * sphere.Radius;
            for (int triangle = 0; triangle < mesh.TriangleCount; triangle++)
            {
                mesh.GetTriangle(triangle, out FPVector3 a, out FPVector3 b, out FPVector3 c);
                FPVector3 closest = CYGTriangleMath.ClosestPointOnTriangle(sphere.Center, a, b, c);
                if (FPVector3.DistanceSquared(closest, sphere.Center) <= radiusSquared)
                {
                    return true;
                }
            }
            return false;
        }

        public static bool Overlaps(CYGStaticMesh mesh, FPCapsule capsule)
        {
            FPAABB bounds = capsule.GetAABB();
            if (!mesh.Bounds.Intersects(bounds))
            {
                return false;
            }

            for (int sample = 0; sample < CapsuleSamples; sample++)
            {
                FP t = FP.FromFraction(sample, CapsuleSamples - 1);
                FPVector3 center = capsule.PointA + (capsule.PointB - capsule.PointA) * t;
                if (Overlaps(mesh, new FPSphere(center, capsule.Radius)))
                {
                    return true;
                }
            }
            return false;
        }
    }
}
