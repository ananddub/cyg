//-----------------------------------------------------------------------------
// CYGPhysicsBody.cs
// Deterministic 3D rigid body state for the CYG physics MVP.
// Zero UnityEngine dependencies. All simulation state is Q32.32 fixed-point.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// A deterministic rigid body.
    /// </summary>
    /// <remarks>
    /// <para>
    /// The body's <see cref="Position"/> is its center of mass and <see cref="Rotation"/>
    /// orients boxes and capsules for collision, queries, mesh contacts and joint
    /// anchors. Ids are assigned by <see cref="CYGPhysicsWorld.AddBody"/> and never
    /// change while the body exists.
    /// </para>
    /// <para>
    /// Rotational state is integrated with a diagonal local inertia tensor.
    /// Contacts, friction and joint anchors apply impulses at their contact point,
    /// so off-centre hits correctly produce torque.
    /// </para>
    /// </remarks>
    [Serializable]
    public sealed class CYGPhysicsBody
    {
        public int Id { get; internal set; } = -1;

        public CYGBodyType Type = CYGBodyType.Static;

        public CYGCollider Collider;

        /// <summary>World-space center of mass.</summary>
        public FPVector3 Position;

        /// <summary>Orientation used for presentation and joint anchors.</summary>
        public FPQuaternion Rotation = FPQuaternion.Identity;

        /// <summary>Linear velocity in world units per second.</summary>
        public FPVector3 LinearVelocity;

        /// <summary>Angular velocity in radians per second.</summary>
        public FPVector3 AngularVelocity;

        /// <summary>Mass in kilograms. Ignored by static and kinematic bodies.</summary>
        public FP Mass;

        /// <summary>
        /// Scalar inertia approximation. Recomputed by <see cref="SetMass"/> and
        /// when the shape is changed through <see cref="CYGPhysicsWorld"/>.
        /// </summary>
        public FP Inertia;

        /// <summary>Diagonal local-space inertia tensor.</summary>
        public FPVector3 InertiaLocal;

        /// <summary>Reciprocal diagonal local-space inertia tensor.</summary>
        public FPVector3 InverseInertiaLocal;

        public FP LinearDamping;

        public FP AngularDamping;

        /// <summary>Restitution in [0, 1]. Pair restitution is the maximum of both.</summary>
        public FP Restitution;

        /// <summary>Coulomb friction coefficient. Pair friction is the geometric mean.</summary>
        public FP Friction = FP.FromFraction(1, 2);

        /// <summary>Multiplier applied to world gravity.</summary>
        public FP GravityScale = FP.One;

        /// <summary>Layer category in [0, 31].</summary>
        public int Layer;

        /// <summary>Bitmask of layers this body interacts with.</summary>
        public uint LayerMask = 0xFFFFFFFFu;

        /// <summary>
        /// Enables conservative sphere CCD against static colliders. Only sphere
        /// shapes are swept. See <see cref="CYGPhysicsWorld"/> limitations.
        /// </summary>
        public bool EnableCcd;

        /// <summary>False after removal from the world. Inactive bodies are skipped.</summary>
        public bool IsActive = true;

        /// <summary>Sleeping dynamic bodies are not integrated and receive no impulses.</summary>
        public bool IsAwake = true;

        /// <summary>Seconds spent below the sleep thresholds.</summary>
        internal FP SleepTimer;

        public bool IsDynamic
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => Type == CYGBodyType.Dynamic;
        }

        public bool IsStatic
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => Type == CYGBodyType.Static;
        }

        public bool IsKinematic
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => Type == CYGBodyType.Kinematic;
        }

        /// <summary>Zero for static and kinematic bodies and for non-positive mass.</summary>
        public FP InverseMass
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get
            {
                if (Type != CYGBodyType.Dynamic || Mass <= FP.Zero)
                    return FP.Zero;
                return FP.One / Mass;
            }
        }

        /// <summary>Zero when inertias are disabled or the body is not dynamic.</summary>
        public FP InverseInertia
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get
            {
                if (Type != CYGBodyType.Dynamic || Inertia <= FP.Zero)
                    return FP.Zero;
                return FP.One / Inertia;
            }
        }

        /// <summary>World-space center of the collider shape.</summary>
        public FPVector3 WorldCenter
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => Collider.GetWorldCenter(Position, Rotation);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPAABB GetWorldAABB()
        {
            return Collider.GetWorldAABB(Position, Rotation);
        }

        /// <summary>
        /// Recomputes the diagonal local inertia tensor from the current shape/mass.
        /// </summary>
        public void RecalculateInertia()
        {
            if (Type != CYGBodyType.Dynamic || Mass <= FP.Zero)
            {
                Inertia = FP.Zero;
                InertiaLocal = FPVector3.Zero;
                InverseInertiaLocal = FPVector3.Zero;
                return;
            }

            CYGShape shape = Collider.Shape;
            if (shape.Type == CYGShapeType.Sphere)
            {
                // Solid sphere: I = 2/5 m r^2 on every axis.
                FP value = Mass * shape.Radius * shape.Radius * FP.FromFraction(2, 5);
                InertiaLocal = new FPVector3(value, value, value);
            }
            else if (shape.Type == CYGShapeType.Box)
            {
                // Solid box with full extents (2h):
                // Ix = m (hy^2 + hz^2) / 3, and likewise for y/z.
                FPVector3 h = shape.HalfExtents;
                InertiaLocal = new FPVector3(
                    Mass * (h.y * h.y + h.z * h.z) * FP.One / FP.FromInt(3),
                    Mass * (h.x * h.x + h.z * h.z) * FP.One / FP.FromInt(3),
                    Mass * (h.x * h.x + h.y * h.y) * FP.One / FP.FromInt(3));
            }
            else
            {
                FPVector3 h = shape.GetBoundingHalfExtents();
                InertiaLocal = new FPVector3(
                    Mass * (h.y * h.y + h.z * h.z) * FP.One / FP.FromInt(3),
                    Mass * (h.x * h.x + h.z * h.z) * FP.One / FP.FromInt(3),
                    Mass * (h.x * h.x + h.y * h.y) * FP.One / FP.FromInt(3));
            }

            Inertia = (InertiaLocal.x + InertiaLocal.y + InertiaLocal.z) * (FP.One / FP.FromInt(3));
            InverseInertiaLocal = new FPVector3(
                InertiaLocal.x > FP.Zero ? FP.One / InertiaLocal.x : FP.Zero,
                InertiaLocal.y > FP.Zero ? FP.One / InertiaLocal.y : FP.Zero,
                InertiaLocal.z > FP.Zero ? FP.One / InertiaLocal.z : FP.Zero);
        }

        /// <summary>Applies the world-space inverse inertia tensor to a torque/impulse.</summary>
        public FPVector3 TransformInverseInertia(FPVector3 value)
        {
            if (Type != CYGBodyType.Dynamic)
                return FPVector3.Zero;
            if (InverseInertiaLocal == FPVector3.Zero)
                return value * InverseInertia;

            FPVector3 local = Rotation == FPQuaternion.Identity
                ? value
                : FPQuaternion.Rotate(Rotation.Conjugated, value);
            local = local * InverseInertiaLocal;
            return Rotation == FPQuaternion.Identity
                ? local
                : FPQuaternion.Rotate(Rotation, local);
        }

        /// <summary>Sets dynamic mass and refreshes the inertia approximation.</summary>
        public void SetMass(FP mass)
        {
            if (mass <= FP.Zero)
                throw new ArgumentOutOfRangeException(nameof(mass), "Dynamic mass must be positive.");
            Mass = mass;
            RecalculateInertia();
            Wake();
        }

        /// <summary>Wakes the body and resets its sleep timer.</summary>
        public void Wake()
        {
            IsAwake = true;
            SleepTimer = FP.Zero;
        }

        /// <summary>Puts a dynamic body to sleep and zeroes its velocities.</summary>
        public void Sleep()
        {
            if (Type != CYGBodyType.Dynamic)
                return;
            IsAwake = false;
            SleepTimer = FP.Zero;
            LinearVelocity = FPVector3.Zero;
            AngularVelocity = FPVector3.Zero;
        }

        /// <summary>Velocity of a world-space point, including angular velocity.</summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public FPVector3 GetPointVelocity(FPVector3 worldPoint)
        {
            return LinearVelocity + FPVector3.Cross(AngularVelocity, worldPoint - Position);
        }

        public void ApplyLinearImpulse(FPVector3 impulse)
        {
            if (Type != CYGBodyType.Dynamic)
                return;
            LinearVelocity += impulse * InverseMass;
            Wake();
        }

        public void ApplyAngularImpulse(FPVector3 impulse)
        {
            if (Type != CYGBodyType.Dynamic)
                return;
            AngularVelocity += TransformInverseInertia(impulse);
            Wake();
        }

        /// <summary>Applies an impulse at a world-space point (linear + angular).</summary>
        public void ApplyImpulseAtPoint(FPVector3 impulse, FPVector3 worldPoint)
        {
            if (Type != CYGBodyType.Dynamic)
                return;
            LinearVelocity += impulse * InverseMass;
            AngularVelocity += TransformInverseInertia(FPVector3.Cross(worldPoint - Position, impulse));
            Wake();
        }

        public void ApplyTorque(FPVector3 torque, FP deltaTime)
        {
            if (Type != CYGBodyType.Dynamic)
                return;
            AngularVelocity += TransformInverseInertia(torque) * deltaTime;
            Wake();
        }

        /// <summary>Creates a static body with infinite mass and zero velocity.</summary>
        public static CYGPhysicsBody CreateStatic(CYGCollider collider)
        {
            CYGPhysicsBody body = new CYGPhysicsBody();
            body.Type = CYGBodyType.Static;
            body.Collider = collider;
            body.Mass = FP.Zero;
            body.IsAwake = true;
            return body;
        }

        /// <summary>Creates a kinematic body: velocity driven, infinite mass.</summary>
        public static CYGPhysicsBody CreateKinematic(CYGCollider collider)
        {
            CYGPhysicsBody body = new CYGPhysicsBody();
            body.Type = CYGBodyType.Kinematic;
            body.Collider = collider;
            body.Mass = FP.Zero;
            body.IsAwake = true;
            return body;
        }

        /// <summary>Creates a dynamic body with the supplied positive mass.</summary>
        public static CYGPhysicsBody CreateDynamic(CYGCollider collider, FP mass)
        {
            if (mass <= FP.Zero)
                throw new ArgumentOutOfRangeException(nameof(mass), "Dynamic mass must be positive.");

            CYGPhysicsBody body = new CYGPhysicsBody();
            body.Type = CYGBodyType.Dynamic;
            body.Collider = collider;
            body.Mass = mass;
            body.IsAwake = true;
            body.RecalculateInertia();
            return body;
        }
    }
}
