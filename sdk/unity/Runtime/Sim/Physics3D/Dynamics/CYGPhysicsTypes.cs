//-----------------------------------------------------------------------------
// CYGPhysicsTypes.cs
// Shape, collider, settings and query/event value types for the deterministic
// CYG 3D rigid body MVP.
// Zero UnityEngine dependencies. All simulation math is Q32.32 fixed-point.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Motion class of a rigid body.
    /// Static bodies never move. Kinematic bodies move by their velocity but have
    /// infinite mass. Dynamic bodies receive gravity, contacts and joint impulses.
    /// </summary>
    public enum CYGBodyType
    {
        Static = 0,
        Kinematic = 1,
        Dynamic = 2
    }

    /// <summary>
    /// Supported collision primitives.
    /// </summary>
    public enum CYGShapeType
    {
        Box = 0,
        Sphere = 1,
        Capsule = 2,
        Mesh = 3
    }

    /// <summary>
    /// Collision shape. Boxes and capsules are oriented by their body rotation.
    /// </summary>
    /// <remarks>
    /// <para>
    /// Boxes use oriented SAT and capsules use the body rotation for their segment
    /// axis. A sphere is rotation invariant. Mesh shapes are world-space static
    /// triangle soup and never rotate. The identity-rotation path keeps the
    /// original AABB/vertical-capsule math for compatibility and determinism.
    /// </para>
    /// <para>
    /// Capsule <see cref="Height"/> is the total height including both caps and must
    /// be at least twice <see cref="Radius"/>.
    /// </para>
    /// </remarks>
    [Serializable]
    public struct CYGShape
    {
        public CYGShapeType Type;
        public FPVector3 HalfExtents;
        public FP Radius;
        public FP Height;

        /// <summary>
        /// Id of the static triangle mesh in <see cref="CYGMeshRegistry"/>. Only
        /// meaningful when <see cref="Type"/> is <see cref="CYGShapeType.Mesh"/>.
        /// </summary>
        public int MeshId;

        public static CYGShape CreateBox(FPVector3 halfExtents)
        {
            return new CYGShape
            {
                Type = CYGShapeType.Box,
                HalfExtents = halfExtents,
                Radius = FP.Zero,
                Height = FP.Zero
            };
        }

        public static CYGShape CreateSphere(FP radius)
        {
            return new CYGShape
            {
                Type = CYGShapeType.Sphere,
                HalfExtents = new FPVector3(radius, radius, radius),
                Radius = radius,
                Height = radius * FP.FromInt(2)
            };
        }

        public static CYGShape CreateCapsule(FP radius, FP height)
        {
            return new CYGShape
            {
                Type = CYGShapeType.Capsule,
                HalfExtents = new FPVector3(radius, FP.Max(radius, height * FP.Half), radius),
                Radius = radius,
                Height = height
            };
        }

        /// <summary>
        /// Creates a static triangle-mesh shape. <paramref name="bounds"/> is the
        /// world-space bounds of the baked mesh; the owning body is positioned at
        /// the bounds center so the sweep-and-prune AABB stays correct.
        /// </summary>
        public static CYGShape CreateMesh(int meshId, FPAABB bounds)
        {
            return new CYGShape
            {
                Type = CYGShapeType.Mesh,
                HalfExtents = bounds.Extents,
                Radius = FP.Zero,
                Height = FP.Zero,
                MeshId = meshId
            };
        }

        /// <summary>Half extents of the shape's axis-aligned bounding box.</summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPVector3 GetBoundingHalfExtents()
        {
            if (Type == CYGShapeType.Box || Type == CYGShapeType.Mesh)
                return HalfExtents;

            if (Type == CYGShapeType.Sphere)
                return new FPVector3(Radius, Radius, Radius);

            FP halfHeight = FP.Max(Radius, Height * FP.Half);
            return new FPVector3(Radius, halfHeight, Radius);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPAABB GetAABB(FPVector3 center)
        {
            FPVector3 half = GetBoundingHalfExtents();
            return new FPAABB(center - half, center + half);
        }

        /// <summary>
        /// Half length of a capsule's internal segment. Always non-negative.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FP GetCapsuleSegmentHalfLength()
        {
            if (Type != CYGShapeType.Capsule)
                return FP.Zero;
            return FP.Max(FP.Zero, Height * FP.Half - Radius);
        }
    }

    /// <summary>
    /// Collider attached to a physics body: a shape, a local center offset and the
    /// trigger flag.
    /// </summary>
    [Serializable]
    public struct CYGCollider
    {
        public CYGShape Shape;

        /// <summary>Local-space offset of the shape center from the body center.</summary>
        public FPVector3 Offset;

        /// <summary>
        /// Trigger colliders generate trigger events but never contact impulses.
        /// </summary>
        public bool IsTrigger;

        public static CYGCollider Box(FPVector3 halfExtents)
        {
            return new CYGCollider { Shape = CYGShape.CreateBox(halfExtents) };
        }

        public static CYGCollider Sphere(FP radius)
        {
            return new CYGCollider { Shape = CYGShape.CreateSphere(radius) };
        }

        public static CYGCollider Capsule(FP radius, FP height)
        {
            return new CYGCollider { Shape = CYGShape.CreateCapsule(radius, height) };
        }

        /// <summary>
        /// Creates a collider for an already-baked static mesh. Mesh shapes are
        /// static-only; the world-space vertices live in the mesh itself.
        /// </summary>
        public static CYGCollider Mesh(CYGStaticMesh mesh)
        {
            if (mesh == null)
                throw new ArgumentNullException(nameof(mesh));
            return new CYGCollider
            {
                Shape = CYGShape.CreateMesh(mesh.Id, mesh.Bounds)
            };
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPVector3 GetWorldCenter(FPVector3 bodyPosition, FPQuaternion bodyRotation)
        {
            if (Offset == FPVector3.Zero || bodyRotation == FPQuaternion.Identity)
                return bodyPosition + Offset;
            return bodyPosition + FPQuaternion.Rotate(bodyRotation, Offset);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPAABB GetWorldAABB(FPVector3 bodyPosition, FPQuaternion bodyRotation)
        {
            return Shape.GetAABB(GetWorldCenter(bodyPosition, bodyRotation));
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPCapsule GetWorldCapsule(FPVector3 center)
        {
            return FPCapsule.CreateVertical(center, Shape.Height, Shape.Radius);
        }

        /// <summary>World capsule with the body's orientation applied to its axis.</summary>
        public FPCapsule GetWorldCapsule(FPVector3 center, FPQuaternion rotation)
        {
            if (rotation == FPQuaternion.Identity)
                return FPCapsule.CreateVertical(center, Shape.Height, Shape.Radius);

            FP half = Shape.GetCapsuleSegmentHalfLength();
            FPVector3 axis = FPQuaternion.Rotate(rotation, FPVector3.Up);
            return new FPCapsule(center - axis * half, center + axis * half, Shape.Radius);
        }
    }

    /// <summary>
    /// Resolved contact between two bodies. <see cref="Normal"/> points from
    /// <see cref="BodyA"/> towards <see cref="BodyB"/>; BodyA has the smaller id.
    /// </summary>
    [Serializable]
    public struct CYGContact
    {
        public int BodyA;
        public int BodyB;
        public FPVector3 Point;
        public FPVector3 Normal;
        public FP Depth;
    }

    /// <summary>
    /// Event kinds emitted by a <see cref="CYGPhysicsWorld"/> step.
    /// </summary>
    public enum CYGPhysicsEventType
    {
        CollisionEnter = 0,
        CollisionStay = 1,
        CollisionExit = 2,
        TriggerEnter = 3,
        TriggerStay = 4,
        TriggerExit = 5
    }

    /// <summary>
    /// Ordered physics event. Collisions use BodyA &lt; BodyB. Trigger events put
    /// the trigger in BodyA (or the lower trigger id when both are triggers).
    /// </summary>
    [Serializable]
    public struct CYGPhysicsEvent
    {
        public CYGPhysicsEventType Type;
        public int BodyA;
        public int BodyB;
        public FPVector3 Point;
        public FPVector3 Normal;
        public FP Depth;

        public bool IsTriggerEvent
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => Type >= CYGPhysicsEventType.TriggerEnter;
        }
    }

    /// <summary>
    /// Hit returned by raycast, sphere cast and capsule cast queries.
    /// </summary>
    [Serializable]
    public struct CYGPhysicsHit
    {
        public bool Hit;
        public int BodyId;
        public FP Distance;
        public FPVector3 Point;
        public FPVector3 Normal;
        public FP Fraction;
        public bool IsTrigger;

        public static readonly CYGPhysicsHit None = new CYGPhysicsHit
        {
            Hit = false,
            BodyId = -1,
            Distance = FP.MaxValue,
            Point = FPVector3.Zero,
            Normal = FPVector3.Up,
            Fraction = FP.One,
            IsTrigger = false
        };
    }

    /// <summary>
    /// Result of an overlap query. <see cref="Distance"/> is the gap from the query
    /// volume to the collider surface, clamped to zero when overlapping.
    /// </summary>
    [Serializable]
    public struct CYGOverlapResult
    {
        public int BodyId;
        public FP Distance;
        public bool IsTrigger;
    }

    /// <summary>
    /// Tunable deterministic simulation settings. Values are read on every step.
    /// </summary>
    [Serializable]
    public sealed class CYGPhysicsSettings
    {
        /// <summary>World gravity in units per second squared.</summary>
        public FPVector3 Gravity = new FPVector3(FP.Zero, -FP.FromInt(10), FP.Zero);

        /// <summary>Delta time used by Step(). Fixed-step simulation is expected.</summary>
        public FP FixedDeltaTime = FP.FromFraction(1, 60);

        /// <summary>Sequential impulse velocity iterations. Minimum 1.</summary>
        public int VelocityIterations = 8;

        /// <summary>Position (split-impulse) correction iterations. Minimum 1.</summary>
        public int PositionIterations = 3;

        /// <summary>Distance joint velocity and position iterations. Minimum 1.</summary>
        public int JointIterations = 6;

        /// <summary>Approach speed below which restitution is ignored.</summary>
        public FP RestitutionThreshold = FP.FromFraction(1, 2);

        /// <summary>Fraction of excess penetration removed per position iteration.</summary>
        public FP Baumgarte = FP.FromFraction(4, 5);

        /// <summary>Allowed penetration before positional correction starts.</summary>
        public FP PenetrationSlop = FP.FromFraction(1, 1000);

        /// <summary>Position correction fraction for rigid distance joints.</summary>
        public FP JointPositionCorrection = FP.FromFraction(4, 5);

        /// <summary>Linear speed below which a body may sleep.</summary>
        public FP SleepLinearThreshold = FP.FromFraction(1, 50);

        /// <summary>Angular speed below which a body may sleep.</summary>
        public FP SleepAngularThreshold = FP.FromFraction(1, 20);

        /// <summary>Seconds a body must remain slow before it sleeps.</summary>
        public FP SleepDelay = FP.FromFraction(1, 2);

        /// <summary>Enables the deterministic sleeping state machine.</summary>
        public bool EnableSleeping = true;

        /// <summary>Enables conservative sphere CCD for bodies that opt in.</summary>
        public bool EnableCcd = true;

        /// <summary>
        /// Applies contact impulses at the contact point so friction and normal
        /// forces generate angular velocity. Disable only for legacy linear-only
        /// behaviour.
        /// </summary>
        public bool EnableRotationalContacts = true;
    }
}
