//-----------------------------------------------------------------------------
// CYGPhysicsQueries.cs
// Deterministic raycast, sphere cast, capsule cast and overlap queries, plus
// the ICollisionWorld adapter used by FPCharacterController.
// Zero UnityEngine dependencies. All math is Q32.32 fixed-point.
//-----------------------------------------------------------------------------

using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    public sealed partial class CYGPhysicsWorld
    {
        /// <summary>
        /// Returns the nearest raycast hit. Ties are resolved by lower body id.
        /// </summary>
        public CYGPhysicsHit Raycast(
            FPRay ray,
            FP maxDistance,
            uint layerMask = 0xFFFFFFFFu,
            bool includeTriggers = true)
        {
            return RaycastInternal(ray, maxDistance, layerMask, includeTriggers);
        }

        /// <summary>Appends every raycast hit, sorted by distance then body id.</summary>
        public int RaycastAll(
            FPRay ray,
            FP maxDistance,
            List<CYGPhysicsHit> results,
            uint layerMask = 0xFFFFFFFFu,
            bool includeTriggers = true)
        {
            if (results == null)
                throw new ArgumentNullException(nameof(results));

            results.Clear();
            if (maxDistance < FP.Zero)
                return 0;

            for (int id = 0; id < _bodies.Count; id++)
            {
                CYGPhysicsBody body = _bodies[id];
                if (!PassesQueryFilter(body, layerMask, includeTriggers))
                    continue;

                FPRaycastHit shapeHit;
                if (TryRaycastBody(body, ray, maxDistance, out shapeHit))
                    results.Add(MakeHit(body, shapeHit));
            }

            SortHits(results);
            return results.Count;
        }

        /// <summary>
        /// Returns the nearest sphere cast hit. Ties are resolved by lower body id.
        /// </summary>
        public CYGPhysicsHit SphereCast(
            FPSphere sphere,
            FPVector3 direction,
            FP maxDistance,
            uint layerMask = 0xFFFFFFFFu,
            bool includeTriggers = true)
        {
            return SphereCastInternal(sphere, direction, maxDistance, layerMask, includeTriggers);
        }

        /// <summary>
        /// Returns the nearest capsule cast hit. Ties are resolved by lower body id.
        /// </summary>
        public CYGPhysicsHit CapsuleCast(
            FPCapsule capsule,
            FPVector3 direction,
            FP maxDistance,
            uint layerMask = 0xFFFFFFFFu,
            bool includeTriggers = true)
        {
            return CapsuleCastInternal(capsule, direction, maxDistance, layerMask, includeTriggers);
        }

        /// <summary>
        /// Appends bodies whose collider intersects the sphere, sorted by distance
        /// from the query center to the collider surface, then body id.
        /// </summary>
        public int OverlapSphere(
            FPSphere sphere,
            List<CYGOverlapResult> results,
            uint layerMask = 0xFFFFFFFFu,
            bool includeTriggers = true)
        {
            if (results == null)
                throw new ArgumentNullException(nameof(results));

            results.Clear();
            if (sphere.Radius < FP.Zero)
                return 0;

            for (int id = 0; id < _bodies.Count; id++)
            {
                CYGPhysicsBody body = _bodies[id];
                if (!PassesQueryFilter(body, layerMask, includeTriggers))
                    continue;

                FP distance = DistanceFromPointToShape(sphere.Center, body);
                if (distance > sphere.Radius)
                    continue;

                CYGOverlapResult result = default(CYGOverlapResult);
                result.BodyId = body.Id;
                result.Distance = distance;
                result.IsTrigger = body.Collider.IsTrigger;
                results.Add(result);
            }

            SortOverlaps(results);
            return results.Count;
        }

        /// <summary>
        /// Appends bodies whose AABB intersects the query AABB, sorted by AABB gap
        /// then body id.
        /// </summary>
        public int OverlapAABB(
            FPAABB aabb,
            List<CYGOverlapResult> results,
            uint layerMask = 0xFFFFFFFFu,
            bool includeTriggers = true)
        {
            if (results == null)
                throw new ArgumentNullException(nameof(results));

            results.Clear();

            for (int id = 0; id < _bodies.Count; id++)
            {
                CYGPhysicsBody body = _bodies[id];
                if (!PassesQueryFilter(body, layerMask, includeTriggers))
                    continue;

                FPAABB bodyAabb = body.GetWorldAABB();
                if (!aabb.Intersects(bodyAabb))
                    continue;

                CYGOverlapResult result = default(CYGOverlapResult);
                result.BodyId = body.Id;
                result.Distance = AabbGap(aabb, bodyAabb);
                result.IsTrigger = body.Collider.IsTrigger;
                results.Add(result);
            }

            SortOverlaps(results);
            return results.Count;
        }

        /// <summary>
        /// True when the capsule intersects any non-trigger collider. Used by the
        /// deterministic character controller for depenetration checks.
        /// </summary>
        public bool CheckCapsuleOverlap(FPCapsule capsule)
        {
            for (int id = 0; id < _bodies.Count; id++)
            {
                CYGPhysicsBody body = _bodies[id];
                if (body == null || !body.IsActive || body.Collider.IsTrigger)
                    continue;

                CYGShapeType type = body.Collider.Shape.Type;
                FPVector3 center = body.WorldCenter;

                if (type == CYGShapeType.Box)
                {
                    if (body.Rotation == FPQuaternion.Identity)
                    {
                        if (capsule.Intersects(body.Collider.Shape.GetAABB(center)))
                            return true;
                    }
                    else
                    {
                        FPCapsule localCapsule = ToLocalCapsule(capsule, center, body.Rotation);
                        if (localCapsule.Intersects(body.Collider.Shape.GetAABB(FPVector3.Zero)))
                            return true;
                    }
                }
                else if (type == CYGShapeType.Sphere)
                {
                    if (capsule.Intersects(new FPSphere(center, body.Collider.Shape.Radius)))
                        return true;
                }
                else if (type == CYGShapeType.Mesh)
                {
                    CYGStaticMesh mesh = CYGMeshRegistry.Get(body.Collider.Shape.MeshId);
                    if (mesh != null && mesh.Overlaps(capsule))
                        return true;
                }
                else
                {
                    if (capsule.Intersects(body.Collider.GetWorldCapsule(center, body.Rotation)))
                        return true;
                }
            }

            return false;
        }

        private CYGPhysicsHit RaycastInternal(FPRay ray, FP maxDistance, uint layerMask, bool includeTriggers)
        {
            CYGPhysicsHit best = CYGPhysicsHit.None;
            if (maxDistance < FP.Zero)
                return best;

            for (int id = 0; id < _bodies.Count; id++)
            {
                CYGPhysicsBody body = _bodies[id];
                if (!PassesQueryFilter(body, layerMask, includeTriggers))
                    continue;

                FPRaycastHit shapeHit;
                if (!TryRaycastBody(body, ray, maxDistance, out shapeHit))
                    continue;

                if (IsBetterHit(shapeHit.Distance, body.Id, best))
                    best = MakeHit(body, shapeHit);
            }

            return best;
        }

        private CYGPhysicsHit SphereCastInternal(
            FPSphere sphere,
            FPVector3 direction,
            FP maxDistance,
            uint layerMask,
            bool includeTriggers)
        {
            CYGPhysicsHit best = CYGPhysicsHit.None;
            if (maxDistance < FP.Zero)
                return best;

            for (int id = 0; id < _bodies.Count; id++)
            {
                CYGPhysicsBody body = _bodies[id];
                if (!PassesQueryFilter(body, layerMask, includeTriggers))
                    continue;

                FPRaycastHit shapeHit = FPRaycastHit.None;
                bool hit = false;
                CYGShapeType type = body.Collider.Shape.Type;
                FPVector3 center = body.WorldCenter;

                if (type == CYGShapeType.Box)
                {
                    if (body.Rotation == FPQuaternion.Identity)
                    {
                        hit = FPRaycast.SphereCast(sphere, direction, maxDistance, body.Collider.Shape.GetAABB(center), out shapeHit);
                    }
                    else
                    {
                        FPSphere localSphere = new FPSphere(
                            CYGOrientedBox.WorldToLocal(body.Rotation, sphere.Center - center),
                            sphere.Radius);
                        FPVector3 localDirection = CYGOrientedBox.WorldToLocal(body.Rotation, direction);
                        hit = FPRaycast.SphereCast(
                            localSphere,
                            localDirection,
                            maxDistance,
                            body.Collider.Shape.GetAABB(FPVector3.Zero),
                            out shapeHit);
                        if (hit)
                            shapeHit = ToWorldHit(shapeHit, center, body.Rotation);
                    }
                }
                else if (type == CYGShapeType.Sphere)
                {
                    hit = FPRaycast.SphereCast(sphere, direction, maxDistance, new FPSphere(center, body.Collider.Shape.Radius), out shapeHit);
                }
                else if (type == CYGShapeType.Mesh)
                {
                    CYGStaticMesh mesh = CYGMeshRegistry.Get(body.Collider.Shape.MeshId);
                    hit = mesh != null && mesh.SphereCast(sphere, direction, maxDistance, out shapeHit);
                }
                else
                {
                    hit = FPRaycast.SphereCast(sphere, direction, maxDistance, body.Collider.GetWorldCapsule(center, body.Rotation), out shapeHit);
                }

                if (hit && IsBetterHit(shapeHit.Distance, body.Id, best))
                    best = MakeHit(body, shapeHit);
            }

            return best;
        }

        private CYGPhysicsHit CapsuleCastInternal(
            FPCapsule capsule,
            FPVector3 direction,
            FP maxDistance,
            uint layerMask,
            bool includeTriggers)
        {
            CYGPhysicsHit best = CYGPhysicsHit.None;
            if (maxDistance < FP.Zero)
                return best;

            for (int id = 0; id < _bodies.Count; id++)
            {
                CYGPhysicsBody body = _bodies[id];
                if (!PassesQueryFilter(body, layerMask, includeTriggers))
                    continue;

                FPRaycastHit shapeHit = FPRaycastHit.None;
                bool hit = false;
                CYGShapeType type = body.Collider.Shape.Type;
                FPVector3 center = body.WorldCenter;

                if (type == CYGShapeType.Box)
                {
                    if (body.Rotation == FPQuaternion.Identity)
                    {
                        hit = FPRaycast.CapsuleCast(capsule, direction, maxDistance, body.Collider.Shape.GetAABB(center), out shapeHit);
                    }
                    else
                    {
                        FPCapsule localCapsule = ToLocalCapsule(capsule, center, body.Rotation);
                        FPVector3 localDirection = CYGOrientedBox.WorldToLocal(body.Rotation, direction);
                        hit = FPRaycast.CapsuleCast(
                            localCapsule,
                            localDirection,
                            maxDistance,
                            body.Collider.Shape.GetAABB(FPVector3.Zero),
                            out shapeHit);
                        if (hit)
                            shapeHit = ToWorldHit(shapeHit, center, body.Rotation);
                    }
                }
                else if (type == CYGShapeType.Sphere)
                {
                    hit = FPRaycast.CapsuleCast(capsule, direction, maxDistance, new FPSphere(center, body.Collider.Shape.Radius), out shapeHit);
                }
                else if (type == CYGShapeType.Mesh)
                {
                    CYGStaticMesh mesh = CYGMeshRegistry.Get(body.Collider.Shape.MeshId);
                    hit = mesh != null && mesh.CapsuleCast(capsule, direction, maxDistance, out shapeHit);
                }
                else
                {
                    hit = FPRaycast.CapsuleCast(capsule, direction, maxDistance, body.Collider.GetWorldCapsule(center, body.Rotation), out shapeHit);
                }

                if (hit && IsBetterHit(shapeHit.Distance, body.Id, best))
                    best = MakeHit(body, shapeHit);
            }

            return best;
        }

        bool ICollisionWorld.CapsuleCast(FPCapsule capsule, FPVector3 direction, FP maxDistance, out FPRaycastHit hit)
        {
            CYGPhysicsHit query = CapsuleCastInternal(capsule, direction, maxDistance, 0xFFFFFFFFu, false);
            hit = ToRaycastHit(query);
            return query.Hit;
        }

        bool ICollisionWorld.SphereCast(FPSphere sphere, FPVector3 direction, FP maxDistance, out FPRaycastHit hit)
        {
            CYGPhysicsHit query = SphereCastInternal(sphere, direction, maxDistance, 0xFFFFFFFFu, false);
            hit = ToRaycastHit(query);
            return query.Hit;
        }

        bool ICollisionWorld.Raycast(FPRay ray, FP maxDistance, out FPRaycastHit hit)
        {
            CYGPhysicsHit query = RaycastInternal(ray, maxDistance, 0xFFFFFFFFu, false);
            hit = ToRaycastHit(query);
            return query.Hit;
        }

        private static bool TryRaycastBody(CYGPhysicsBody body, FPRay ray, FP maxDistance, out FPRaycastHit hit)
        {
            CYGShapeType type = body.Collider.Shape.Type;
            FPVector3 center = body.WorldCenter;

            if (type == CYGShapeType.Box)
            {
                if (body.Rotation == FPQuaternion.Identity)
                    return FPRaycast.RaycastAABB(ray, body.Collider.Shape.GetAABB(center), maxDistance, out hit);

                FPRay localRay = new FPRay(
                    CYGOrientedBox.WorldToLocal(body.Rotation, ray.Origin - center),
                    CYGOrientedBox.WorldToLocal(body.Rotation, ray.Direction));
                bool localHit = FPRaycast.RaycastAABB(
                    localRay,
                    body.Collider.Shape.GetAABB(FPVector3.Zero),
                    maxDistance,
                    out hit);
                if (localHit)
                    hit = ToWorldHit(hit, center, body.Rotation);
                return localHit;
            }
            if (type == CYGShapeType.Sphere)
                return FPRaycast.RaycastSphere(ray, new FPSphere(center, body.Collider.Shape.Radius), maxDistance, out hit);
            if (type == CYGShapeType.Mesh)
            {
                CYGStaticMesh mesh = CYGMeshRegistry.Get(body.Collider.Shape.MeshId);
                if (mesh != null)
                {
                    return mesh.Raycast(ray, maxDistance, out hit);
                }
                hit = default;
                return false;
            }
            return FPRaycast.RaycastCapsule(ray, body.Collider.GetWorldCapsule(center, body.Rotation), maxDistance, out hit);
        }

        private static bool PassesQueryFilter(CYGPhysicsBody body, uint layerMask, bool includeTriggers)
        {
            if (body == null || !body.IsActive)
                return false;
            if (!includeTriggers && body.Collider.IsTrigger)
                return false;
            return (layerMask & (1u << body.Layer)) != 0;
        }

        private static bool IsBetterHit(FP distance, int bodyId, CYGPhysicsHit best)
        {
            if (!best.Hit)
                return true;
            if (distance < best.Distance)
                return true;
            return distance == best.Distance && bodyId < best.BodyId;
        }

        private static CYGPhysicsHit MakeHit(CYGPhysicsBody body, FPRaycastHit shapeHit)
        {
            CYGPhysicsHit hit = default(CYGPhysicsHit);
            hit.Hit = shapeHit.Hit;
            hit.BodyId = body.Id;
            hit.Distance = shapeHit.Distance;
            hit.Point = shapeHit.Point;
            hit.Normal = shapeHit.Normal;
            hit.Fraction = shapeHit.Fraction;
            hit.IsTrigger = body.Collider.IsTrigger;
            return hit;
        }

        private static FPRaycastHit ToRaycastHit(CYGPhysicsHit query)
        {
            if (!query.Hit)
                return FPRaycastHit.None;

            FPRaycastHit hit = default(FPRaycastHit);
            hit.Hit = true;
            hit.Distance = query.Distance;
            hit.Point = query.Point;
            hit.Normal = query.Normal;
            hit.Fraction = query.Fraction;
            return hit;
        }

        private static FPCapsule ToLocalCapsule(FPCapsule capsule, FPVector3 center, FPQuaternion rotation)
        {
            return new FPCapsule(
                CYGOrientedBox.WorldToLocal(rotation, capsule.PointA - center),
                CYGOrientedBox.WorldToLocal(rotation, capsule.PointB - center),
                capsule.Radius);
        }

        private static FPRaycastHit ToWorldHit(FPRaycastHit hit, FPVector3 center, FPQuaternion rotation)
        {
            hit.Point = center + CYGOrientedBox.LocalToWorld(rotation, hit.Point);
            hit.Normal = CYGOrientedBox.LocalToWorld(rotation, hit.Normal);
            return hit;
        }

        private static FP DistanceFromPointToShape(FPVector3 point, CYGPhysicsBody body)
        {
            CYGShapeType type = body.Collider.Shape.Type;
            FPVector3 center = body.WorldCenter;

            if (type == CYGShapeType.Box)
            {
                if (body.Rotation == FPQuaternion.Identity)
                {
                    FPAABB aabb = body.Collider.Shape.GetAABB(center);
                    return FP.Sqrt(aabb.DistanceSquared(point));
                }

                FPVector3 closest = CYGOrientedBox.ClosestPoint(
                    point,
                    center,
                    body.Collider.Shape.HalfExtents,
                    body.Rotation);
                return FPVector3.Distance(point, closest);
            }

            if (type == CYGShapeType.Sphere)
            {
                FP distance = FPVector3.Distance(point, center) - body.Collider.Shape.Radius;
                return FP.Max(FP.Zero, distance);
            }

            if (type == CYGShapeType.Mesh)
            {
                CYGStaticMesh mesh = CYGMeshRegistry.Get(body.Collider.Shape.MeshId);
                return mesh != null ? FP.Sqrt(mesh.DistanceSquared(point)) : FP.MaxValue;
            }

            FPCapsule capsule = body.Collider.GetWorldCapsule(center, body.Rotation);
            FP segmentDistance = FP.Sqrt(capsule.DistanceSquaredToSegment(point));
            return FP.Max(FP.Zero, segmentDistance - capsule.Radius);
        }

        private static FP AabbGap(FPAABB a, FPAABB b)
        {
            FP squared = FP.Zero;

            if (a.Max.x < b.Min.x)
            {
                FP d = b.Min.x - a.Max.x;
                squared += d * d;
            }
            else if (b.Max.x < a.Min.x)
            {
                FP d = a.Min.x - b.Max.x;
                squared += d * d;
            }

            if (a.Max.y < b.Min.y)
            {
                FP d = b.Min.y - a.Max.y;
                squared += d * d;
            }
            else if (b.Max.y < a.Min.y)
            {
                FP d = a.Min.y - b.Max.y;
                squared += d * d;
            }

            if (a.Max.z < b.Min.z)
            {
                FP d = b.Min.z - a.Max.z;
                squared += d * d;
            }
            else if (b.Max.z < a.Min.z)
            {
                FP d = a.Min.z - b.Max.z;
                squared += d * d;
            }

            return FP.Sqrt(squared);
        }

        private static void SortHits(List<CYGPhysicsHit> results)
        {
            for (int i = 1; i < results.Count; i++)
            {
                CYGPhysicsHit item = results[i];
                int j = i - 1;
                while (j >= 0)
                {
                    CYGPhysicsHit previous = results[j];
                    bool after = previous.Distance > item.Distance ||
                                 (previous.Distance == item.Distance && previous.BodyId > item.BodyId);
                    if (!after)
                        break;
                    results[j + 1] = previous;
                    j--;
                }
                results[j + 1] = item;
            }
        }

        private static void SortOverlaps(List<CYGOverlapResult> results)
        {
            for (int i = 1; i < results.Count; i++)
            {
                CYGOverlapResult item = results[i];
                int j = i - 1;
                while (j >= 0)
                {
                    CYGOverlapResult previous = results[j];
                    bool after = previous.Distance > item.Distance ||
                                 (previous.Distance == item.Distance && previous.BodyId > item.BodyId);
                    if (!after)
                        break;
                    results[j + 1] = previous;
                    j--;
                }
                results[j + 1] = item;
            }
        }
    }
}
