//-----------------------------------------------------------------------------
// CYGOrientedBox.cs
// Deterministic oriented-box primitives, axes, bounds and sphere/capsule contacts.
// Zero UnityEngine dependencies. All math is Q32.32 fixed-point.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Oriented-box helpers shared by the narrowphase, queries and mesh casts.
    /// A box's local axes are the rotated world axes of the body; half extents
    /// stay in local space.
    /// </summary>
    internal static partial class CYGOrientedBox
    {
        internal static readonly FP Epsilon = FP.FromFraction(1, 1000000);

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static void GetAxes(FPQuaternion rotation, out FPVector3 x, out FPVector3 y, out FPVector3 z)
        {
            if (rotation == FPQuaternion.Identity)
            {
                x = FPVector3.Right;
                y = FPVector3.Up;
                z = FPVector3.Forward;
                return;
            }

            x = FPQuaternion.Rotate(rotation, FPVector3.Right);
            y = FPQuaternion.Rotate(rotation, FPVector3.Up);
            z = FPQuaternion.Rotate(rotation, FPVector3.Forward);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 GetAxis(FPQuaternion rotation, int axis)
        {
            if (rotation == FPQuaternion.Identity)
            {
                if (axis == 0) return FPVector3.Right;
                if (axis == 1) return FPVector3.Up;
                return FPVector3.Forward;
            }

            FPVector3 local = axis == 0 ? FPVector3.Right : axis == 1 ? FPVector3.Up : FPVector3.Forward;
            return FPQuaternion.Rotate(rotation, local);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FP GetExtent(FPVector3 halfExtents, int axis)
        {
            if (axis == 0) return halfExtents.x;
            if (axis == 1) return halfExtents.y;
            return halfExtents.z;
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 WorldToLocal(FPQuaternion rotation, FPVector3 point)
        {
            return rotation == FPQuaternion.Identity
                ? point
                : FPQuaternion.Rotate(rotation.Conjugated, point);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 LocalToWorld(FPQuaternion rotation, FPVector3 point)
        {
            return rotation == FPQuaternion.Identity ? point : FPQuaternion.Rotate(rotation, point);
        }

        public static FPVector3 ClosestPoint(
            FPVector3 point,
            FPVector3 center,
            FPVector3 halfExtents,
            FPQuaternion rotation)
        {
            FPVector3 local = WorldToLocal(rotation, point - center);
            FPVector3 clamped = new FPVector3(
                FP.Clamp(local.x, -halfExtents.x, halfExtents.x),
                FP.Clamp(local.y, -halfExtents.y, halfExtents.y),
                FP.Clamp(local.z, -halfExtents.z, halfExtents.z));
            return center + LocalToWorld(rotation, clamped);
        }

        public static FPAABB GetAABB(FPVector3 center, FPVector3 halfExtents, FPQuaternion rotation)
        {
            if (rotation == FPQuaternion.Identity)
                return new FPAABB(center - halfExtents, center + halfExtents);

            GetAxes(rotation, out FPVector3 x, out FPVector3 y, out FPVector3 z);
            FPVector3 extent = new FPVector3(
                FP.Abs(x.x) * halfExtents.x + FP.Abs(y.x) * halfExtents.y + FP.Abs(z.x) * halfExtents.z,
                FP.Abs(x.y) * halfExtents.x + FP.Abs(y.y) * halfExtents.y + FP.Abs(z.y) * halfExtents.z,
                FP.Abs(x.z) * halfExtents.x + FP.Abs(y.z) * halfExtents.y + FP.Abs(z.z) * halfExtents.z);
            return new FPAABB(center - extent, center + extent);
        }

        public static bool SphereBox(
            FPVector3 sphereCenter,
            FP radius,
            FPVector3 boxCenter,
            FPVector3 halfExtents,
            FPQuaternion boxRotation,
            out FPVector3 normalBoxToSphere,
            out FPVector3 point,
            out FP depth)
        {
            normalBoxToSphere = FPVector3.Up;
            point = sphereCenter;
            depth = FP.Zero;

            FPVector3 local = WorldToLocal(boxRotation, sphereCenter - boxCenter);
            FPVector3 clamped = new FPVector3(
                FP.Clamp(local.x, -halfExtents.x, halfExtents.x),
                FP.Clamp(local.y, -halfExtents.y, halfExtents.y),
                FP.Clamp(local.z, -halfExtents.z, halfExtents.z));
            FPVector3 delta = local - clamped;
            FP distanceSquared = delta.SqrMagnitude;

            if (distanceSquared > radius * radius)
                return false;

            if (distanceSquared > Epsilon * Epsilon)
            {
                FP distance = FP.Sqrt(distanceSquared);
                FPVector3 normalLocal = delta / distance;
                normalBoxToSphere = LocalToWorld(boxRotation, normalLocal);
                depth = radius - distance;
                point = boxCenter + LocalToWorld(boxRotation, clamped);
                return true;
            }

            // Center inside: push out through the nearest face with a deterministic
            // x/y/z and min/max tie-break.
            FP best = halfExtents.x - FP.Abs(local.x);
            int axis = 0;
            int sign = local.x >= FP.Zero ? 1 : -1;

            FP candidate = halfExtents.y - FP.Abs(local.y);
            if (candidate < best)
            {
                best = candidate;
                axis = 1;
                sign = local.y >= FP.Zero ? 1 : -1;
            }

            candidate = halfExtents.z - FP.Abs(local.z);
            if (candidate < best)
            {
                best = candidate;
                axis = 2;
                sign = local.z >= FP.Zero ? 1 : -1;
            }

            FPVector3 faceNormalLocal = axis == 0
                ? new FPVector3(sign, FP.Zero, FP.Zero)
                : axis == 1 ? new FPVector3(FP.Zero, sign, FP.Zero) : new FPVector3(FP.Zero, FP.Zero, sign);
            FPVector3 faceLocal = local;
            if (axis == 0) faceLocal.x = halfExtents.x * sign;
            else if (axis == 1) faceLocal.y = halfExtents.y * sign;
            else faceLocal.z = halfExtents.z * sign;

            normalBoxToSphere = LocalToWorld(boxRotation, faceNormalLocal);
            point = boxCenter + LocalToWorld(boxRotation, faceLocal);
            depth = radius + best;
            return true;
        }

        /// <summary>
        /// Capsule versus oriented box. The capsule axis comes from its body
        /// rotation; up to four deepest sampled sphere contacts are returned.
        /// </summary>
        public static bool CapsuleBox(
            CYGPhysicsBody capsule,
            FPVector3 capsuleCenter,
            FPVector3 boxCenter,
            FPVector3 halfExtents,
            FPQuaternion boxRotation,
            out CYGOrientedContacts contacts)
        {
            contacts = default(CYGOrientedContacts);

            FPVector3 axis = GetAxis(capsule.Rotation, 1);
            FP segmentHalf = capsule.Collider.Shape.GetCapsuleSegmentHalfLength();
            FP radius = capsule.Collider.Shape.Radius;
            const int samples = 9;
            FP bestDepth = FP.Zero;

            for (int i = 0; i < samples; i++)
            {
                FP t = samples == 1 ? FP.Half : FP.FromFraction(i, samples - 1);
                FPVector3 sample = capsuleCenter + axis * (segmentHalf * (t * FP.FromInt(2) - FP.One));
                if (!SphereBox(sample, radius, boxCenter, halfExtents, boxRotation, out FPVector3 normal, out FPVector3 point, out FP depth))
                    continue;

                contacts.Add(point, depth);
                if (depth > bestDepth)
                {
                    bestDepth = depth;
                    contacts.Normal = normal;
                }
            }

            return contacts.Count > 0;
        }
    }
}
