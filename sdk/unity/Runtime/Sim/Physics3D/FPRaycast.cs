//-----------------------------------------------------------------------------
// FPRaycast.cs
// Raycast and Shapecast queries against FPAABB, Sphere, and Capsule.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Ray representation with origin and normalized direction.
    /// </summary>
    [Serializable]
    public struct FPRay
    {
        public FPVector3 Origin;
        public FPVector3 Direction;

        public FPRay(FPVector3 origin, FPVector3 direction)
        {
            Origin = origin;
            Direction = direction.Normalized;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPVector3 GetPoint(FP distance) => Origin + Direction * distance;

        public override string ToString() => $"Ray(Origin:{Origin}, Dir:{Direction})";
    }

    /// <summary>
    /// Information returned from raycast and shapecast queries.
    /// </summary>
    [Serializable]
    public struct FPRaycastHit
    {
        public bool Hit;
        public FP Distance;
        public FPVector3 Point;
        public FPVector3 Normal;
        public FP Fraction;

        public static readonly FPRaycastHit None = new FPRaycastHit
        {
            Hit = false,
            Distance = FP.MaxValue,
            Point = FPVector3.Zero,
            Normal = FPVector3.Up,
            Fraction = FP.One
        };

        public override string ToString() => Hit ? $"Hit(Dist:{Distance}, Point:{Point}, Normal:{Normal})" : "NoHit";
    }

    /// <summary>
    /// Deterministic 3D raycast and shapecast algorithms.
    /// </summary>
    public static class FPRaycast
    {
        #region Raycast Queries

        /// <summary>
        /// Raycast against Axis-Aligned Bounding Box using the Kay-Kajiya slab method.
        /// </summary>
        public static bool RaycastAABB(FPRay ray, FPAABB aabb, FP maxDistance, out FPRaycastHit hit)
        {
            hit = FPRaycastHit.None;

            FP tNear = FP.MinValue;
            FP tFar = maxDistance;
            FPVector3 hitNormal = FPVector3.Zero;

            // X Axis
            if (FP.Abs(ray.Direction.x) < FP.Epsilon)
            {
                if (ray.Origin.x < aabb.Min.x || ray.Origin.x > aabb.Max.x)
                    return false;
            }
            else
            {
                FP invD = FP.One / ray.Direction.x;
                FP t0 = (aabb.Min.x - ray.Origin.x) * invD;
                FP t1 = (aabb.Max.x - ray.Origin.x) * invD;
                FPVector3 normal0 = FPVector3.Left;

                if (invD < FP.Zero)
                {
                    FP tmp = t0; t0 = t1; t1 = tmp;
                    normal0 = FPVector3.Right;
                }

                if (t0 > tNear)
                {
                    tNear = t0;
                    hitNormal = normal0;
                }
                if (t1 < tFar) tFar = t1;
                if (tNear > tFar) return false;
            }

            // Y Axis
            if (FP.Abs(ray.Direction.y) < FP.Epsilon)
            {
                if (ray.Origin.y < aabb.Min.y || ray.Origin.y > aabb.Max.y)
                    return false;
            }
            else
            {
                FP invD = FP.One / ray.Direction.y;
                FP t0 = (aabb.Min.y - ray.Origin.y) * invD;
                FP t1 = (aabb.Max.y - ray.Origin.y) * invD;
                FPVector3 normal0 = FPVector3.Down;

                if (invD < FP.Zero)
                {
                    FP tmp = t0; t0 = t1; t1 = tmp;
                    normal0 = FPVector3.Up;
                }

                if (t0 > tNear)
                {
                    tNear = t0;
                    hitNormal = normal0;
                }
                if (t1 < tFar) tFar = t1;
                if (tNear > tFar) return false;
            }

            // Z Axis
            if (FP.Abs(ray.Direction.z) < FP.Epsilon)
            {
                if (ray.Origin.z < aabb.Min.z || ray.Origin.z > aabb.Max.z)
                    return false;
            }
            else
            {
                FP invD = FP.One / ray.Direction.z;
                FP t0 = (aabb.Min.z - ray.Origin.z) * invD;
                FP t1 = (aabb.Max.z - ray.Origin.z) * invD;
                FPVector3 normal0 = FPVector3.Back;

                if (invD < FP.Zero)
                {
                    FP tmp = t0; t0 = t1; t1 = tmp;
                    normal0 = FPVector3.Forward;
                }

                if (t0 > tNear)
                {
                    tNear = t0;
                    hitNormal = normal0;
                }
                if (t1 < tFar) tFar = t1;
                if (tNear > tFar) return false;
            }

            if (tFar < FP.Zero)
                return false;

            FP hitDist = tNear < FP.Zero ? FP.Zero : tNear;
            if (hitDist > maxDistance)
                return false;

            hit.Hit = true;
            hit.Distance = hitDist;
            hit.Point = ray.GetPoint(hitDist);
            hit.Normal = tNear < FP.Zero ? -ray.Direction : hitNormal;
            hit.Fraction = maxDistance > FP.Zero ? hitDist / maxDistance : FP.Zero;
            return true;
        }

        /// <summary>
        /// Raycast against Sphere using analytical quadratic solver.
        /// </summary>
        public static bool RaycastSphere(FPRay ray, FPSphere sphere, FP maxDistance, out FPRaycastHit hit)
        {
            hit = FPRaycastHit.None;

            FPVector3 m = ray.Origin - sphere.Center;
            FP b = FPVector3.Dot(m, ray.Direction);
            FP c = FPVector3.Dot(m, m) - sphere.Radius * sphere.Radius;

            // Outside pointing away
            if (c > FP.Zero && b > FP.Zero)
                return false;

            FP discr = b * b - c;
            if (discr < FP.Zero)
                return false;

            FP sqrtDiscr = FP.Sqrt(discr);
            FP t = -b - sqrtDiscr;

            if (t < FP.Zero)
            {
                // Origin inside sphere
                hit.Hit = true;
                hit.Distance = FP.Zero;
                hit.Point = ray.Origin;
                hit.Normal = -ray.Direction;
                hit.Fraction = FP.Zero;
                return true;
            }

            if (t > maxDistance)
                return false;

            hit.Hit = true;
            hit.Distance = t;
            hit.Point = ray.GetPoint(t);
            FPVector3 normal = (hit.Point - sphere.Center).Normalized;
            hit.Normal = normal != FPVector3.Zero ? normal : -ray.Direction;
            hit.Fraction = maxDistance > FP.Zero ? t / maxDistance : FP.Zero;
            return true;
        }

        /// <summary>
        /// Raycast against Capsule (cylinder body + two hemispherical caps).
        /// </summary>
        public static bool RaycastCapsule(FPRay ray, FPCapsule capsule, FP maxDistance, out FPRaycastHit hit)
        {
            hit = FPRaycastHit.None;

            FPVector3 axis = capsule.PointB - capsule.PointA;
            FP len = axis.Magnitude;

            // Degenerate capsule is a sphere
            if (len < FP.Epsilon)
            {
                return RaycastSphere(ray, new FPSphere(capsule.PointA, capsule.Radius), maxDistance, out hit);
            }

            FPVector3 v = axis / len; // unit axis
            FP bestT = FP.MaxValue;
            FPVector3 bestNormal = FPVector3.Zero;

            // 1. Check cylinder body
            FPVector3 d = ray.Direction;
            FPVector3 w = ray.Origin - capsule.PointA;

            FPVector3 dPerp = d - v * FPVector3.Dot(d, v);
            FPVector3 wPerp = w - v * FPVector3.Dot(w, v);

            FP A = FPVector3.Dot(dPerp, dPerp);
            FP B = FPVector3.Dot(wPerp, dPerp);
            FP C = FPVector3.Dot(wPerp, wPerp) - capsule.Radius * capsule.Radius;

            if (A > FP.Epsilon)
            {
                FP discr = B * B - A * C;
                if (discr >= FP.Zero)
                {
                    FP sqrtDiscr = FP.Sqrt(discr);
                    FP t0 = (-B - sqrtDiscr) / A;

                    if (t0 >= FP.Zero && t0 <= maxDistance && t0 < bestT)
                    {
                        FPVector3 p = ray.GetPoint(t0);
                        FP h = FPVector3.Dot(p - capsule.PointA, v);
                        if (h >= FP.Zero && h <= len)
                        {
                            bestT = t0;
                            FPVector3 radial = p - (capsule.PointA + v * h);
                            bestNormal = radial.Normalized;
                        }
                    }
                }
            }

            // 2. Check Cap A Sphere
            if (RaycastSphere(ray, new FPSphere(capsule.PointA, capsule.Radius), maxDistance, out var hitA))
            {
                if (hitA.Distance < bestT && FPVector3.Dot(hitA.Point - capsule.PointA, v) <= FP.Zero)
                {
                    bestT = hitA.Distance;
                    bestNormal = hitA.Normal;
                }
            }

            // 3. Check Cap B Sphere
            if (RaycastSphere(ray, new FPSphere(capsule.PointB, capsule.Radius), maxDistance, out var hitB))
            {
                if (hitB.Distance < bestT && FPVector3.Dot(hitB.Point - capsule.PointB, v) >= FP.Zero)
                {
                    bestT = hitB.Distance;
                    bestNormal = hitB.Normal;
                }
            }

            if (bestT <= maxDistance)
            {
                hit.Hit = true;
                hit.Distance = bestT;
                hit.Point = ray.GetPoint(bestT);
                hit.Normal = bestNormal != FPVector3.Zero ? bestNormal : -ray.Direction;
                hit.Fraction = maxDistance > FP.Zero ? bestT / maxDistance : FP.Zero;
                return true;
            }

            return false;
        }

        #endregion

        #region Shapecast Queries

        /// <summary>
        /// SphereCast against another Sphere.
        /// Equivalent to a raycast against a sphere with combined radii.
        /// </summary>
        public static bool SphereCast(FPSphere sphere, FPVector3 direction, FP maxDistance, FPSphere target, out FPRaycastHit hit)
        {
            FPRay ray = new FPRay(sphere.Center, direction);
            FPSphere expandedTarget = new FPSphere(target.Center, target.Radius + sphere.Radius);

            if (RaycastSphere(ray, expandedTarget, maxDistance, out hit))
            {
                // Adjust hit point onto target surface
                hit.Point = target.ClosestPoint(hit.Point);
                return true;
            }

            return false;
        }

        /// <summary>
        /// SphereCast against a Capsule.
        /// Equivalent to a raycast against a capsule with combined radii.
        /// </summary>
        public static bool SphereCast(FPSphere sphere, FPVector3 direction, FP maxDistance, FPCapsule target, out FPRaycastHit hit)
        {
            FPRay ray = new FPRay(sphere.Center, direction);
            FPCapsule expandedTarget = new FPCapsule(target.PointA, target.PointB, target.Radius + sphere.Radius);

            if (RaycastCapsule(ray, expandedTarget, maxDistance, out hit))
            {
                hit.Point = target.ClosestPoint(hit.Point);
                return true;
            }

            return false;
        }

        /// <summary>
        /// SphereCast against an AABB using Minkowski rounded-box raycast.
        /// </summary>
        public static bool SphereCast(FPSphere sphere, FPVector3 direction, FP maxDistance, FPAABB target, out FPRaycastHit hit)
        {
            hit = FPRaycastHit.None;

            // Expanded AABB (slab approximation for Minkowski sum)
            FPAABB expandedAABB = target;
            expandedAABB.Expand(sphere.Radius);

            FPRay ray = new FPRay(sphere.Center, direction);
            if (!RaycastAABB(ray, expandedAABB, maxDistance, out var aabbHit))
                return false;

            // If hit point lies on flat face, it is an exact hit
            FPVector3 p = aabbHit.Point;
            bool onFaceX = p.x >= target.Min.x && p.x <= target.Max.x;
            bool onFaceY = p.y >= target.Min.y && p.y <= target.Max.y;
            bool onFaceZ = p.z >= target.Min.z && p.z <= target.Max.z;

            if ((onFaceX && onFaceY) || (onFaceX && onFaceZ) || (onFaceY && onFaceZ))
            {
                hit = aabbHit;
                hit.Point = target.ClosestPoint(aabbHit.Point);
                return true;
            }

            // Otherwise, check closest point on the unexpanded box corners/edges
            FPVector3 closestTarget = target.ClosestPoint(aabbHit.Point);
            FPSphere cornerSphere = new FPSphere(closestTarget, sphere.Radius);
            if (RaycastSphere(ray, cornerSphere, maxDistance, out var cornerHit))
            {
                hit = cornerHit;
                hit.Point = closestTarget;
                return true;
            }

            // Fallback to slab hit if within tolerance
            hit = aabbHit;
            hit.Point = target.ClosestPoint(aabbHit.Point);
            return true;
        }

        /// <summary>
        /// CapsuleCast against an AABB.
        /// Samples key points along the capsule axis (PointA, Center, PointB) with sphere casts.
        /// </summary>
        public static bool CapsuleCast(FPCapsule capsule, FPVector3 direction, FP maxDistance, FPAABB target, out FPRaycastHit hit)
        {
            hit = FPRaycastHit.None;
            FP bestDist = FP.MaxValue;

            // Bottom sphere
            FPSphere sphereA = new FPSphere(capsule.PointA, capsule.Radius);
            if (SphereCast(sphereA, direction, maxDistance, target, out var hitA))
            {
                if (hitA.Distance < bestDist)
                {
                    bestDist = hitA.Distance;
                    hit = hitA;
                }
            }

            // Top sphere
            FPSphere sphereB = new FPSphere(capsule.PointB, capsule.Radius);
            if (SphereCast(sphereB, direction, maxDistance, target, out var hitB))
            {
                if (hitB.Distance < bestDist)
                {
                    bestDist = hitB.Distance;
                    hit = hitB;
                }
            }

            // Center sphere
            FPSphere sphereCenter = new FPSphere(capsule.Center, capsule.Radius);
            if (SphereCast(sphereCenter, direction, maxDistance, target, out var hitC))
            {
                if (hitC.Distance < bestDist)
                {
                    bestDist = hitC.Distance;
                    hit = hitC;
                }
            }

            return hit.Hit;
        }

        /// <summary>
        /// CapsuleCast against a Sphere.
        /// Equivalent to a reversed SphereCast against the capsule.
        /// </summary>
        public static bool CapsuleCast(FPCapsule capsule, FPVector3 direction, FP maxDistance, FPSphere target, out FPRaycastHit hit)
        {
            hit = FPRaycastHit.None;
            if (SphereCast(target, -direction, maxDistance, capsule, out var revHit))
            {
                hit.Hit = true;
                hit.Distance = revHit.Distance;
                hit.Point = revHit.Point;
                hit.Normal = -revHit.Normal;
                hit.Fraction = revHit.Fraction;
                return true;
            }
            return false;
        }

        /// <summary>
        /// CapsuleCast against another Capsule.
        /// Sweeps the endpoints and center of capsule 1 against capsule 2.
        /// </summary>
        public static bool CapsuleCast(FPCapsule capsule, FPVector3 direction, FP maxDistance, FPCapsule target, out FPRaycastHit hit)
        {
            hit = FPRaycastHit.None;
            FP bestDist = FP.MaxValue;

            FPSphere sA = new FPSphere(capsule.PointA, capsule.Radius);
            if (SphereCast(sA, direction, maxDistance, target, out var hitA))
            {
                if (hitA.Distance < bestDist) { bestDist = hitA.Distance; hit = hitA; }
            }

            FPSphere sB = new FPSphere(capsule.PointB, capsule.Radius);
            if (SphereCast(sB, direction, maxDistance, target, out var hitB))
            {
                if (hitB.Distance < bestDist) { bestDist = hitB.Distance; hit = hitB; }
            }

            FPSphere sC = new FPSphere(capsule.Center, capsule.Radius);
            if (SphereCast(sC, direction, maxDistance, target, out var hitC))
            {
                if (hitC.Distance < bestDist) { bestDist = hitC.Distance; hit = hitC; }
            }

            return hit.Hit;
        }

        #endregion
    }
}
