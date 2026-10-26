//-----------------------------------------------------------------------------
// CYGPhysics2DBody.cs
// Fixed-point 2D rigid body data and deterministic shape factories.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    /// <summary>Simulation behaviour of a 2D body.</summary>
    public enum CYGPhysics2DBodyType : byte
    {
        Static = 0,
        Kinematic = 1,
        Dynamic = 2
    }

    /// <summary>Supported 2D collider shapes. Boxes are oriented by Body.Angle.</summary>
    public enum CYGPhysics2DShapeType : byte
    {
        Circle = 0,
        Box = 1
    }

    /// <summary>
    /// Deterministic 2D rigid body. Position is the center of mass and the shape
    /// center. Boxes are oriented by <see cref="Angle"/> (radians, counter
    /// clockwise). All persistent state is fixed point.
    ///
    /// This MVP has no continuous collision detection: very fast circles can
    /// tunnel through thin static boxes. Use a small fixed timestep when bullets
    /// must be reliable.
    /// </summary>
    [Serializable]
    public struct CYGPhysics2DBody
    {
        /// <summary>Stable, monotonically increasing id assigned by the world.</summary>
        public int Id;

        /// <summary>False once the body has been destroyed. Managed by the world.</summary>
        public bool Active;

        public CYGPhysics2DBodyType Type;
        public CYGPhysics2DShapeType Shape;

        public FPVector2 Position;

        /// <summary>Orientation in radians, counter clockwise. Circles ignore it.</summary>
        public FP Angle;

        public FPVector2 Velocity;

        /// <summary>Angular velocity in radians per second.</summary>
        public FP AngularVelocity;

        /// <summary>Circle radius. Zero for boxes.</summary>
        public FP Radius;

        /// <summary>Half size of a box in local space. Zero for circles.</summary>
        public FPVector2 HalfExtents;

        /// <summary>Mass in kilograms. Infinite bodies store zero.</summary>
        public FP Mass;

        /// <summary>Cached 1 / Mass, or zero for static and kinematic bodies.</summary>
        public FP InverseMass;

        /// <summary>Rotational inertia. Infinite bodies store zero.</summary>
        public FP Inertia;

        /// <summary>Cached 1 / Inertia, or zero for static and kinematic bodies.</summary>
        public FP InverseInertia;

        /// <summary>Coulomb friction coefficient, combined with the geometric mean.</summary>
        public FP Friction;

        /// <summary>Bounciness in [0, 1], combined with max().</summary>
        public FP Restitution;

        /// <summary>Linear velocity decay per second (0 means no damping).</summary>
        public FP LinearDamping;

        /// <summary>Angular velocity decay per second (0 means no damping).</summary>
        public FP AngularDamping;

        /// <summary>Per-body multiplier applied to world gravity (dynamic bodies only).</summary>
        public FP GravityScale;

        /// <summary>Layer index in [0, 31].</summary>
        public int Layer;

        /// <summary>Bit mask of layers this body can interact with. -1 means all.</summary>
        public int CollisionMask;

        /// <summary>Trigger bodies detect overlaps and fire callbacks but never receive impulses.</summary>
        public bool IsTrigger;

        /// <summary>Sleeping dynamic bodies are excluded from integration and solving.</summary>
        public bool IsAwake;

        /// <summary>Consecutive slow frames counted by the world before sleeping.</summary>
        public int SleepFrames;

        public bool IsDynamic
        {
            get { return Type == CYGPhysics2DBodyType.Dynamic; }
        }

        public bool IsStatic
        {
            get { return Type == CYGPhysics2DBodyType.Static; }
        }

        /// <summary>Creates a dynamic circle with unit mass and default material.</summary>
        public static CYGPhysics2DBody Circle(FPVector2 position, FP radius)
        {
            return Circle(position, radius, CYGPhysics2DBodyType.Dynamic, FP.One);
        }

        /// <summary>Creates a circle with unit mass and default material.</summary>
        public static CYGPhysics2DBody Circle(FPVector2 position, FP radius, CYGPhysics2DBodyType type)
        {
            return Circle(position, radius, type, FP.One);
        }

        /// <summary>Creates a circle with explicit mass (ignored for infinite bodies).</summary>
        public static CYGPhysics2DBody Circle(FPVector2 position, FP radius, CYGPhysics2DBodyType type, FP mass)
        {
            CYGPhysics2DBody body = new CYGPhysics2DBody();
            body.Type = type;
            body.Shape = CYGPhysics2DShapeType.Circle;
            body.Position = position;
            body.Radius = radius;
            body.Mass = mass;
            body.Friction = FP.FromFraction(1, 2);
            body.Restitution = FP.Zero;
            body.GravityScale = FP.One;
            body.Layer = 0;
            body.CollisionMask = -1;
            body.IsAwake = true;
            return body;
        }

        /// <summary>Creates a dynamic box with unit mass and default material.</summary>
        public static CYGPhysics2DBody Box(FPVector2 position, FPVector2 halfExtents)
        {
            return Box(position, halfExtents, FP.Zero, CYGPhysics2DBodyType.Dynamic, FP.One);
        }

        /// <summary>Creates a box with unit mass and default material.</summary>
        public static CYGPhysics2DBody Box(FPVector2 position, FPVector2 halfExtents, CYGPhysics2DBodyType type)
        {
            return Box(position, halfExtents, FP.Zero, type, FP.One);
        }

        /// <summary>Creates a box with an orientation and explicit mass.</summary>
        public static CYGPhysics2DBody Box(
            FPVector2 position,
            FPVector2 halfExtents,
            CYGPhysics2DBodyType type,
            FP mass)
        {
            return Box(position, halfExtents, FP.Zero, type, mass);
        }

        /// <summary>Creates a rotated dynamic box with unit mass.</summary>
        public static CYGPhysics2DBody Box(FPVector2 position, FPVector2 halfExtents, FP angle)
        {
            return Box(position, halfExtents, angle, CYGPhysics2DBodyType.Dynamic, FP.One);
        }

        /// <summary>Creates a rotated box with explicit mass (ignored for infinite bodies).</summary>
        public static CYGPhysics2DBody Box(
            FPVector2 position,
            FPVector2 halfExtents,
            FP angle,
            CYGPhysics2DBodyType type,
            FP mass)
        {
            CYGPhysics2DBody body = new CYGPhysics2DBody();
            body.Type = type;
            body.Shape = CYGPhysics2DShapeType.Box;
            body.Position = position;
            body.Angle = angle;
            body.HalfExtents = halfExtents;
            body.Mass = mass;
            body.Friction = FP.FromFraction(1, 2);
            body.Restitution = FP.Zero;
            body.GravityScale = FP.One;
            body.Layer = 0;
            body.CollisionMask = -1;
            body.IsAwake = true;
            return body;
        }

        /// <summary>Rotates a local-space vector into world space.</summary>
        public FPVector2 RotateVector(FPVector2 local)
        {
            FP cos = FP.Cos(Angle);
            FP sin = FP.Sin(Angle);
            return new FPVector2(cos * local.x - sin * local.y, sin * local.x + cos * local.y);
        }

        /// <summary>Transforms a local-space point into world space.</summary>
        public FPVector2 TransformPoint(FPVector2 local)
        {
            return Position + RotateVector(local);
        }

        /// <summary>Transforms a world-space point into local space.</summary>
        public FPVector2 InverseTransformPoint(FPVector2 world)
        {
            FP cos = FP.Cos(Angle);
            FP sin = FP.Sin(Angle);
            FPVector2 delta = world - Position;
            return new FPVector2(cos * delta.x + sin * delta.y, -sin * delta.x + cos * delta.y);
        }

        /// <summary>Axis-aligned bounds of the body at its current pose.</summary>
        public CYGPhysics2DAABB GetAABB()
        {
            CYGPhysics2DAABB bounds;
            if (Shape == CYGPhysics2DShapeType.Circle)
            {
                bounds.Min = new FPVector2(Position.x - Radius, Position.y - Radius);
                bounds.Max = new FPVector2(Position.x + Radius, Position.y + Radius);
                return bounds;
            }

            if (Angle == FP.Zero)
            {
                bounds.Min = new FPVector2(Position.x - HalfExtents.x, Position.y - HalfExtents.y);
                bounds.Max = new FPVector2(Position.x + HalfExtents.x, Position.y + HalfExtents.y);
                return bounds;
            }

            FP cos = FP.Abs(FP.Cos(Angle));
            FP sin = FP.Abs(FP.Sin(Angle));
            FP extentX = cos * HalfExtents.x + sin * HalfExtents.y;
            FP extentY = sin * HalfExtents.x + cos * HalfExtents.y;

            bounds.Min = new FPVector2(Position.x - extentX, Position.y - extentY);
            bounds.Max = new FPVector2(Position.x + extentX, Position.y + extentY);
            return bounds;
        }

        public FPVector2 GetMin()
        {
            return GetAABB().Min;
        }

        public FPVector2 GetMax()
        {
            return GetAABB().Max;
        }

        /// <summary>Zero-allocation checksum of the simulated body state.</summary>
        public void ComputeChecksum(ref uint crc)
        {
            CYGChecksum.HashInt(ref crc, Id);
            CYGChecksum.HashByte(ref crc, (byte)Type);
            CYGChecksum.HashByte(ref crc, (byte)Shape);
            CYGChecksum.HashLong(ref crc, Position.x.RawValue);
            CYGChecksum.HashLong(ref crc, Position.y.RawValue);
            CYGChecksum.HashLong(ref crc, Angle.RawValue);
            CYGChecksum.HashLong(ref crc, Velocity.x.RawValue);
            CYGChecksum.HashLong(ref crc, Velocity.y.RawValue);
            CYGChecksum.HashLong(ref crc, AngularVelocity.RawValue);
            CYGChecksum.HashLong(ref crc, Radius.RawValue);
            CYGChecksum.HashLong(ref crc, HalfExtents.x.RawValue);
            CYGChecksum.HashLong(ref crc, HalfExtents.y.RawValue);
            CYGChecksum.HashLong(ref crc, Mass.RawValue);
            CYGChecksum.HashLong(ref crc, InverseMass.RawValue);
            CYGChecksum.HashLong(ref crc, Inertia.RawValue);
            CYGChecksum.HashLong(ref crc, InverseInertia.RawValue);
            CYGChecksum.HashLong(ref crc, Friction.RawValue);
            CYGChecksum.HashLong(ref crc, Restitution.RawValue);
            CYGChecksum.HashLong(ref crc, LinearDamping.RawValue);
            CYGChecksum.HashLong(ref crc, AngularDamping.RawValue);
            CYGChecksum.HashLong(ref crc, GravityScale.RawValue);
            CYGChecksum.HashInt(ref crc, Layer);
            CYGChecksum.HashInt(ref crc, CollisionMask);
            CYGChecksum.HashByte(ref crc, IsTrigger ? (byte)1 : (byte)0);
            CYGChecksum.HashByte(ref crc, IsAwake ? (byte)1 : (byte)0);
            CYGChecksum.HashInt(ref crc, SleepFrames);
        }
    }
}
