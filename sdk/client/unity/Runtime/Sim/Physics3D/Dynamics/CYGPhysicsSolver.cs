//-----------------------------------------------------------------------------
// CYGPhysicsSolver.cs
// Fixed-iteration sequential impulse contact solver, split-impulse positional
// correction and deterministic distance/spring joint passes.
// Zero UnityEngine dependencies. All math is Q32.32 fixed-point.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    public sealed partial class CYGPhysicsWorld
    {
        /// <summary>
        /// Cached contact impulses from the previous step, keyed by the ordered
        /// body pair. Re-applying them before solving (warm starting) gives a
        /// resting stack its supporting impulses immediately, so a fixed low
        /// iteration count still converges instead of slowly sinking.
        /// </summary>
        internal struct CYGWarmManifold
        {
            public long Key;
            public FPVector3 Normal;
            public int PointCount;
            public CYGContactPoint Point0;
            public CYGContactPoint Point1;
            public CYGContactPoint Point2;
            public CYGContactPoint Point3;
            public CYGContactPoint Point4;
            public CYGContactPoint Point5;
            public CYGContactPoint Point6;
            public CYGContactPoint Point7;

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
        }

        private CYGWarmManifold[] _warmManifolds = new CYGWarmManifold[0];
        private int _warmManifoldCount;

        private void WakeJointBodies()
        {
            for (int i = 0; i < _joints.Count; i++)
            {
                CYGJoint joint = _joints[i];
                if (joint == null || !joint.IsActive)
                    continue;

                CYGPhysicsBody bodyA = GetBody(joint.BodyA);
                CYGPhysicsBody bodyB = GetBody(joint.BodyB);
                if (bodyA == null || bodyB == null)
                    continue;

                if (IsActiveMover(bodyA) && bodyB.Type == CYGBodyType.Dynamic && !bodyB.IsAwake)
                    bodyB.Wake();
                if (IsActiveMover(bodyB) && bodyA.Type == CYGBodyType.Dynamic && !bodyA.IsAwake)
                    bodyA.Wake();
            }
        }

        private void ApplySpringForces(FP deltaTime)
        {
            for (int i = 0; i < _joints.Count; i++)
            {
                CYGJoint joint = _joints[i];
                if (joint == null || !joint.IsActive || joint.Type != CYGJointType.Spring)
                    continue;

                CYGPhysicsBody bodyA = GetBody(joint.BodyA);
                CYGPhysicsBody bodyB = GetBody(joint.BodyB);
                if (bodyA == null || bodyB == null || !CanSolveJoint(bodyA, bodyB))
                    continue;

                FPVector3 anchorA = CYGJointMath.WorldAnchor(bodyA, joint.AnchorA);
                FPVector3 anchorB = CYGJointMath.WorldAnchor(bodyB, joint.AnchorB);
                FPVector3 delta = anchorB - anchorA;
                FP distance = delta.Magnitude;
                if (distance <= TinyEpsilon)
                    continue;

                FPVector3 normal = delta / distance;
                FP extension = distance - joint.RestLength;
                FP relativeVelocity = FPVector3.Dot(
                    bodyB.GetPointVelocity(anchorB) - bodyA.GetPointVelocity(anchorA),
                    normal);
                FP stiffness = FP.Max(joint.Stiffness, FP.Zero);
                FP damping = FP.Max(joint.Damping, FP.Zero);
                FP force = -(stiffness * extension + damping * relativeVelocity);
                FPVector3 impulse = normal * (force * deltaTime);
                ApplyJointImpulse(bodyA, bodyB, anchorA, anchorB, impulse);
            }
        }

        private void SolveDistanceJointVelocity(FP deltaTime)
        {
            int iterations = Settings.JointIterations < 1 ? 1 : Settings.JointIterations;

            for (int iteration = 0; iteration < iterations; iteration++)
            {
                bool reverse = (iteration & 1) != 0;
                for (int step = 0; step < _joints.Count; step++)
                {
                    int i = reverse ? _joints.Count - 1 - step : step;
                    CYGJoint joint = _joints[i];
                    if (joint == null || !joint.IsActive || joint.Type != CYGJointType.Distance)
                        continue;

                    CYGPhysicsBody bodyA = GetBody(joint.BodyA);
                    CYGPhysicsBody bodyB = GetBody(joint.BodyB);
                    if (bodyA == null || bodyB == null || !CanSolveJoint(bodyA, bodyB))
                        continue;

                    FPVector3 anchorA = CYGJointMath.WorldAnchor(bodyA, joint.AnchorA);
                    FPVector3 anchorB = CYGJointMath.WorldAnchor(bodyB, joint.AnchorB);
                    FPVector3 delta = anchorB - anchorA;
                    FP distance = delta.Magnitude;
                    if (distance <= TinyEpsilon)
                        continue;

                    FPVector3 normal = delta / distance;
                    FP inverseSum = JointInverseMass(bodyA, bodyB, anchorA, anchorB, normal);
                    if (inverseSum <= FP.Zero)
                        continue;
                    FP closingVelocity = FPVector3.Dot(
                        bodyB.GetPointVelocity(anchorB) - bodyA.GetPointVelocity(anchorA),
                        normal);
                    FP lambda = -closingVelocity / inverseSum;
                    ApplyJointImpulse(bodyA, bodyB, anchorA, anchorB, normal * lambda);
                }
            }
        }

        private void SolveDistanceJointPositions()
        {
            int iterations = Settings.JointIterations < 1 ? 1 : Settings.JointIterations;
            FP correctionFactor = FP.Clamp(Settings.JointPositionCorrection, FP.Zero, FP.One);

            for (int iteration = 0; iteration < iterations; iteration++)
            {
                bool reverse = (iteration & 1) != 0;
                for (int step = 0; step < _joints.Count; step++)
                {
                    int i = reverse ? _joints.Count - 1 - step : step;
                    CYGJoint joint = _joints[i];
                    if (joint == null || !joint.IsActive || joint.Type != CYGJointType.Distance)
                        continue;

                    CYGPhysicsBody bodyA = GetBody(joint.BodyA);
                    CYGPhysicsBody bodyB = GetBody(joint.BodyB);
                    if (bodyA == null || bodyB == null || !CanSolveJoint(bodyA, bodyB))
                        continue;

                    FP inverseSum = bodyA.InverseMass + bodyB.InverseMass;
                    if (inverseSum <= FP.Zero)
                        continue;

                    FPVector3 anchorA = CYGJointMath.WorldAnchor(bodyA, joint.AnchorA);
                    FPVector3 anchorB = CYGJointMath.WorldAnchor(bodyB, joint.AnchorB);
                    FPVector3 delta = anchorB - anchorA;
                    FP distance = delta.Magnitude;
                    if (distance <= TinyEpsilon)
                        continue;

                    FPVector3 normal = delta / distance;
                    FP error = (distance - joint.RestLength) * correctionFactor;
                    FP lambda = error / inverseSum;

                    bodyA.Position += normal * (lambda * bodyA.InverseMass);
                    bodyB.Position -= normal * (lambda * bodyB.InverseMass);
                }
            }
        }

        private void PrepareContactSolver()
        {
            for (int i = 0; i < _manifoldCount; i++)
            {
                CYGManifold manifold = _manifolds[i];
                CYGPhysicsBody bodyA = _bodies[manifold.BodyA];
                CYGPhysicsBody bodyB = _bodies[manifold.BodyB];
                if (!CanSolveManifold(bodyA, bodyB))
                {
                    _manifolds[i] = manifold;
                    continue;
                }

                manifold.Tangent1 = ComputeTangent(manifold.Normal);
                manifold.Tangent2 = FPVector3.Cross(manifold.Normal, manifold.Tangent1).Normalized;

                FP restitution = CombineRestitution(bodyA, bodyB);
                FP threshold = FP.Max(Settings.RestitutionThreshold, FP.Zero);
                bool rotational = Settings.EnableRotationalContacts;

                CYGWarmManifold warm;
                bool hasWarm = TryGetWarmManifold(manifold.PairKey, manifold.Normal, manifold.PointCount, out warm);

                FP totalNormal = FP.Zero;
                FP totalTangent1 = FP.Zero;
                FP totalTangent2 = FP.Zero;

                for (int p = 0; p < manifold.PointCount; p++)
                {
                    CYGContactPoint point = manifold.GetPoint(p);
                    point.NormalImpulse = FP.Zero;
                    point.TangentImpulse1 = FP.Zero;
                    point.TangentImpulse2 = FP.Zero;

                    if (hasWarm)
                    {
                        CYGContactPoint previous = warm.GetPoint(p);
                        point.NormalImpulse = previous.NormalImpulse;
                        point.TangentImpulse1 = previous.TangentImpulse1;
                        point.TangentImpulse2 = previous.TangentImpulse2;
                        totalNormal += point.NormalImpulse;
                        totalTangent1 += point.TangentImpulse1;
                        totalTangent2 += point.TangentImpulse2;
                    }

                    FPVector3 relativeVelocity = bodyB.GetPointVelocity(point.Point) - bodyA.GetPointVelocity(point.Point);
                    FP normalVelocity = FPVector3.Dot(relativeVelocity, manifold.Normal);
                    point.RestitutionBias = normalVelocity < -threshold
                        ? -restitution * normalVelocity
                        : FP.Zero;

                    manifold.SetPoint(p, point);
                }

                if (hasWarm)
                {
                    FPVector3 warmImpulse = manifold.Normal * totalNormal
                        + manifold.Tangent1 * totalTangent1
                        + manifold.Tangent2 * totalTangent2;
                    if (warmImpulse != FPVector3.Zero)
                    {
                        FPVector3 anchor = manifold.PointCount > 1 ? ContactCenter(manifold) : manifold.GetPoint(0).Point;
                        ApplyContactImpulse(bodyA, bodyB, anchor, warmImpulse, rotational);
                    }
                }

                _manifolds[i] = manifold;
            }
        }

        private bool TryGetWarmManifold(long key, FPVector3 normal, int pointCount, out CYGWarmManifold warm)
        {
            int low = 0;
            int high = _warmManifoldCount - 1;
            while (low <= high)
            {
                int middle = low + ((high - low) >> 1);
                long value = _warmManifolds[middle].Key;
                if (value == key)
                {
                    CYGWarmManifold candidate = _warmManifolds[middle];
                    if (candidate.PointCount == pointCount
                        && FPVector3.Dot(candidate.Normal, normal) >= FP.FromFraction(99, 100))
                    {
                        warm = candidate;
                        return true;
                    }
                    break;
                }
                if (value < key)
                    low = middle + 1;
                else
                    high = middle - 1;
            }

            warm = default(CYGWarmManifold);
            return false;
        }

        /// <summary>
        /// Stores the accumulated contact impulses so the next step can warm-start.
        /// Entries stay keyed and sorted like the current manifolds.
        /// </summary>
        private void SaveWarmManifolds()
        {
            EnsureArraySize(ref _warmManifolds, _manifoldCount);

            for (int i = 0; i < _manifoldCount; i++)
            {
                CYGManifold manifold = _manifolds[i];
                bool solvable = CanSolveManifold(_bodies[manifold.BodyA], _bodies[manifold.BodyB]);

                CYGWarmManifold warm = default(CYGWarmManifold);
                warm.Key = manifold.PairKey;
                warm.Normal = manifold.Normal;
                warm.PointCount = solvable ? manifold.PointCount : 0;
                if (solvable)
                {
                    for (int p = 0; p < manifold.PointCount; p++)
                        warm.SetPoint(p, manifold.GetPoint(p));
                }

                _warmManifolds[i] = warm;
            }

            _warmManifoldCount = _manifoldCount;
        }

        private void SolveContactVelocity(FP deltaTime)
        {
            int iterations = Settings.VelocityIterations < 1 ? 1 : Settings.VelocityIterations;

            for (int iteration = 0; iteration < iterations; iteration++)
            {
                bool reverse = (iteration & 1) != 0;
                for (int step = 0; step < _manifoldCount; step++)
                {
                    int i = reverse ? _manifoldCount - 1 - step : step;
                    CYGManifold manifold = _manifolds[i];
                    CYGPhysicsBody bodyA = _bodies[manifold.BodyA];
                    CYGPhysicsBody bodyB = _bodies[manifold.BodyB];
                    if (!CanSolveManifold(bodyA, bodyB))
                        continue;

                    FP inverseSum = bodyA.InverseMass + bodyB.InverseMass;
                    if (inverseSum <= FP.Zero)
                        continue;
                    FP friction = CombineFriction(bodyA, bodyB);
                    bool rotational = Settings.EnableRotationalContacts;

                    if (manifold.PointCount > 1)
                        SolveMultiPoint(ref manifold, bodyA, bodyB, friction, rotational);
                    else
                        SolveSinglePoint(ref manifold, bodyA, bodyB, friction, rotational);

                    _manifolds[i] = manifold;
                }
            }
        }

        private static void SolveSinglePoint(
            ref CYGManifold manifold,
            CYGPhysicsBody bodyA,
            CYGPhysicsBody bodyB,
            FP friction,
            bool rotational)
        {
            CYGContactPoint point = manifold.GetPoint(0);
            FPVector3 contactPoint = point.Point;

            FPVector3 relativeVelocity = bodyB.GetPointVelocity(contactPoint) - bodyA.GetPointVelocity(contactPoint);
            FP normalVelocity = FPVector3.Dot(relativeVelocity, manifold.Normal);
            FP normalMass = ContactMass(bodyA, bodyB, contactPoint, manifold.Normal, rotational);
            FP lambda = -(normalVelocity - point.RestitutionBias) * normalMass;

            FP previous = point.NormalImpulse;
            FP accumulated = previous + lambda;
            if (accumulated < FP.Zero)
                accumulated = FP.Zero;
            FP normalDelta = accumulated - previous;
            point.NormalImpulse = accumulated;

            if (normalDelta != FP.Zero)
            {
                ApplyContactImpulse(bodyA, bodyB, contactPoint, manifold.Normal * normalDelta, rotational);
                relativeVelocity = bodyB.GetPointVelocity(contactPoint) - bodyA.GetPointVelocity(contactPoint);
            }

            FP frictionLimit = friction * accumulated;
            if (frictionLimit > FP.Zero)
            {
                FP tangentMass = ContactMass(bodyA, bodyB, contactPoint, manifold.Tangent1, rotational);
                FP tangentVelocity = FPVector3.Dot(relativeVelocity, manifold.Tangent1);
                FP tangentLambda = -tangentVelocity * tangentMass;
                previous = point.TangentImpulse1;
                accumulated = FP.Clamp(previous + tangentLambda, -frictionLimit, frictionLimit);
                FP tangentDelta = accumulated - previous;
                point.TangentImpulse1 = accumulated;

                if (tangentDelta != FP.Zero)
                {
                    ApplyContactImpulse(bodyA, bodyB, contactPoint, manifold.Tangent1 * tangentDelta, rotational);
                    relativeVelocity = bodyB.GetPointVelocity(contactPoint) - bodyA.GetPointVelocity(contactPoint);
                }

                tangentMass = ContactMass(bodyA, bodyB, contactPoint, manifold.Tangent2, rotational);
                tangentVelocity = FPVector3.Dot(relativeVelocity, manifold.Tangent2);
                tangentLambda = -tangentVelocity * tangentMass;
                previous = point.TangentImpulse2;
                accumulated = FP.Clamp(previous + tangentLambda, -frictionLimit, frictionLimit);
                tangentDelta = accumulated - previous;
                point.TangentImpulse2 = accumulated;

                if (tangentDelta != FP.Zero)
                    ApplyContactImpulse(bodyA, bodyB, contactPoint, manifold.Tangent2 * tangentDelta, rotational);
            }

            manifold.SetPoint(0, point);
        }

        /// <summary>
        /// Resolves a multi-point face manifold at its centre of pressure. Solving
        /// the points independently biases the first corner and injects spurious
        /// torque into resting stacks; a centre impulse keeps a flat contact
        /// torque-free while still producing torque when the patch is offset from
        /// the centre of mass (ledges, slopes and edges).
        /// </summary>
        private static void SolveMultiPoint(
            ref CYGManifold manifold,
            CYGPhysicsBody bodyA,
            CYGPhysicsBody bodyB,
            FP friction,
            bool rotational)
        {
            int count = manifold.PointCount;
            FP inverseCount = FP.One / FP.FromInt(count);
            FPVector3 center = ContactCenter(manifold);

            FP previousNormal = FP.Zero;
            FP previousTangent1 = FP.Zero;
            FP previousTangent2 = FP.Zero;
            FP restitutionBias = FP.Zero;
            for (int p = 0; p < count; p++)
            {
                CYGContactPoint point = manifold.GetPoint(p);
                previousNormal += point.NormalImpulse;
                previousTangent1 += point.TangentImpulse1;
                previousTangent2 += point.TangentImpulse2;
                restitutionBias += point.RestitutionBias;
            }
            restitutionBias *= inverseCount;

            FPVector3 relativeVelocity = bodyB.GetPointVelocity(center) - bodyA.GetPointVelocity(center);
            FP normalVelocity = FPVector3.Dot(relativeVelocity, manifold.Normal);
            FP normalMass = ContactMass(bodyA, bodyB, center, manifold.Normal, rotational);
            FP lambda = -(normalVelocity - restitutionBias) * normalMass;

            FP accumulated = previousNormal + lambda;
            if (accumulated < FP.Zero)
                accumulated = FP.Zero;
            FP normalDelta = accumulated - previousNormal;
            if (normalDelta != FP.Zero)
            {
                ApplyContactImpulse(bodyA, bodyB, center, manifold.Normal * normalDelta, rotational);
                relativeVelocity = bodyB.GetPointVelocity(center) - bodyA.GetPointVelocity(center);
            }

            FP frictionLimit = friction * accumulated;
            FP tangent1 = previousTangent1;
            FP tangent2 = previousTangent2;
            if (frictionLimit > FP.Zero)
            {
                FP tangentMass = ContactMass(bodyA, bodyB, center, manifold.Tangent1, rotational);
                FP tangentVelocity = FPVector3.Dot(relativeVelocity, manifold.Tangent1);
                FP tangentLambda = -tangentVelocity * tangentMass;
                FP next = FP.Clamp(tangent1 + tangentLambda, -frictionLimit, frictionLimit);
                FP tangentDelta = next - tangent1;
                tangent1 = next;

                if (tangentDelta != FP.Zero)
                {
                    ApplyContactImpulse(bodyA, bodyB, center, manifold.Tangent1 * tangentDelta, rotational);
                    relativeVelocity = bodyB.GetPointVelocity(center) - bodyA.GetPointVelocity(center);
                }

                tangentMass = ContactMass(bodyA, bodyB, center, manifold.Tangent2, rotational);
                tangentVelocity = FPVector3.Dot(relativeVelocity, manifold.Tangent2);
                tangentLambda = -tangentVelocity * tangentMass;
                next = FP.Clamp(tangent2 + tangentLambda, -frictionLimit, frictionLimit);
                tangentDelta = next - tangent2;
                tangent2 = next;

                if (tangentDelta != FP.Zero)
                    ApplyContactImpulse(bodyA, bodyB, center, manifold.Tangent2 * tangentDelta, rotational);
            }

            FP perPointNormal = accumulated * inverseCount;
            FP perPointTangent1 = tangent1 * inverseCount;
            FP perPointTangent2 = tangent2 * inverseCount;
            for (int p = 0; p < count; p++)
            {
                CYGContactPoint point = manifold.GetPoint(p);
                point.NormalImpulse = perPointNormal;
                point.TangentImpulse1 = perPointTangent1;
                point.TangentImpulse2 = perPointTangent2;
                manifold.SetPoint(p, point);
            }
        }

        private static FPVector3 ContactCenter(CYGManifold manifold)
        {
            if (manifold.PointCount <= 0)
                return FPVector3.Zero;

            FPVector3 sum = FPVector3.Zero;
            for (int p = 0; p < manifold.PointCount; p++)
                sum += manifold.GetPoint(p).Point;
            return sum * (FP.One / FP.FromInt(manifold.PointCount));
        }

        private void CorrectContactPositions()
        {
            int iterations = Settings.PositionIterations < 1 ? 1 : Settings.PositionIterations;
            FP correctionFactor = FP.Clamp(Settings.Baumgarte, FP.Zero, FP.One);
            FP slop = FP.Max(Settings.PenetrationSlop, FP.Zero);

            for (int iteration = 0; iteration < iterations; iteration++)
            {
                bool reverse = (iteration & 1) != 0;
                for (int step = 0; step < _manifoldCount; step++)
                {
                    int i = reverse ? _manifoldCount - 1 - step : step;
                    CYGManifold manifold = _manifolds[i];
                    CYGPhysicsBody bodyA = _bodies[manifold.BodyA];
                    CYGPhysicsBody bodyB = _bodies[manifold.BodyB];
                    if (!CanSolveManifold(bodyA, bodyB))
                        continue;

                    FP inverseSum = bodyA.InverseMass + bodyB.InverseMass;
                    if (inverseSum <= FP.Zero)
                        continue;

                    // Recompute the overlap from the current poses. Earlier pairs in
                    // this pass may have moved either body, so the depth captured at
                    // broadphase time is stale; using it would leave coupled stacks
                    // (A-B, B-C) with residual penetration that ratchets every step.
                    CYGManifold current;
                    if (!CYGPhysicsNarrowphase.TryGetManifold(bodyA, bodyB, out current))
                        continue;

                    FP excess = current.MaxDepth - slop;
                    if (excess <= FP.Zero)
                        continue;

                    FP correction = excess * correctionFactor;
                    FP lambda = correction / inverseSum;

                    bodyA.Position -= current.Normal * (lambda * bodyA.InverseMass);
                    bodyB.Position += current.Normal * (lambda * bodyB.InverseMass);
                }
            }
        }

        private static bool CanSolveManifold(CYGPhysicsBody bodyA, CYGPhysicsBody bodyB)
        {
            if (bodyA.Type == CYGBodyType.Dynamic && bodyA.IsAwake)
                return true;
            if (bodyB.Type == CYGBodyType.Dynamic && bodyB.IsAwake)
                return true;
            return false;
        }

        private static bool CanSolveJoint(CYGPhysicsBody bodyA, CYGPhysicsBody bodyB)
        {
            return CanSolveManifold(bodyA, bodyB);
        }

        private static FP ContactMass(
            CYGPhysicsBody bodyA,
            CYGPhysicsBody bodyB,
            FPVector3 contactPoint,
            FPVector3 direction,
            bool rotational)
        {
            FP inverse = bodyA.InverseMass + bodyB.InverseMass;
            if (rotational)
            {
                FPVector3 rA = contactPoint - bodyA.Position;
                FPVector3 rB = contactPoint - bodyB.Position;
                FPVector3 crossA = FPVector3.Cross(rA, direction);
                FPVector3 crossB = FPVector3.Cross(rB, direction);
                inverse += FPVector3.Dot(crossA, bodyA.TransformInverseInertia(crossA));
                inverse += FPVector3.Dot(crossB, bodyB.TransformInverseInertia(crossB));
            }

            return inverse > FP.Zero ? FP.One / inverse : FP.Zero;
        }

        private static void ApplyContactImpulse(
            CYGPhysicsBody bodyA,
            CYGPhysicsBody bodyB,
            FPVector3 contactPoint,
            FPVector3 impulse,
            bool rotational)
        {
            if (rotational)
            {
                if (bodyA.Type == CYGBodyType.Dynamic && bodyA.IsAwake)
                {
                    bodyA.LinearVelocity -= impulse * bodyA.InverseMass;
                    bodyA.AngularVelocity -= bodyA.TransformInverseInertia(
                        FPVector3.Cross(contactPoint - bodyA.Position, impulse));
                }
                if (bodyB.Type == CYGBodyType.Dynamic && bodyB.IsAwake)
                {
                    bodyB.LinearVelocity += impulse * bodyB.InverseMass;
                    bodyB.AngularVelocity += bodyB.TransformInverseInertia(
                        FPVector3.Cross(contactPoint - bodyB.Position, impulse));
                }
                return;
            }

            if (bodyA.Type == CYGBodyType.Dynamic && bodyA.IsAwake)
                bodyA.LinearVelocity -= impulse * bodyA.InverseMass;
            if (bodyB.Type == CYGBodyType.Dynamic && bodyB.IsAwake)
                bodyB.LinearVelocity += impulse * bodyB.InverseMass;
        }

        private static FP JointInverseMass(
            CYGPhysicsBody bodyA,
            CYGPhysicsBody bodyB,
            FPVector3 anchorA,
            FPVector3 anchorB,
            FPVector3 direction)
        {
            FP inverse = bodyA.InverseMass + bodyB.InverseMass;
            FPVector3 rA = anchorA - bodyA.Position;
            FPVector3 rB = anchorB - bodyB.Position;
            FPVector3 crossA = FPVector3.Cross(rA, direction);
            FPVector3 crossB = FPVector3.Cross(rB, direction);
            inverse += FPVector3.Dot(crossA, bodyA.TransformInverseInertia(crossA));
            inverse += FPVector3.Dot(crossB, bodyB.TransformInverseInertia(crossB));
            return inverse;
        }

        private static void ApplyJointImpulse(
            CYGPhysicsBody bodyA,
            CYGPhysicsBody bodyB,
            FPVector3 anchorA,
            FPVector3 anchorB,
            FPVector3 impulse)
        {
            if (bodyA.Type == CYGBodyType.Dynamic && bodyA.IsAwake)
            {
                bodyA.LinearVelocity -= impulse * bodyA.InverseMass;
                bodyA.AngularVelocity -= bodyA.TransformInverseInertia(
                    FPVector3.Cross(anchorA - bodyA.Position, impulse));
            }
            if (bodyB.Type == CYGBodyType.Dynamic && bodyB.IsAwake)
            {
                bodyB.LinearVelocity += impulse * bodyB.InverseMass;
                bodyB.AngularVelocity += bodyB.TransformInverseInertia(
                    FPVector3.Cross(anchorB - bodyB.Position, impulse));
            }
        }

        private static FP CombineRestitution(CYGPhysicsBody bodyA, CYGPhysicsBody bodyB)
        {
            FP restitutionA = FP.Clamp(bodyA.Restitution, FP.Zero, FP.One);
            FP restitutionB = FP.Clamp(bodyB.Restitution, FP.Zero, FP.One);
            return FP.Max(restitutionA, restitutionB);
        }

        private static FP CombineFriction(CYGPhysicsBody bodyA, CYGPhysicsBody bodyB)
        {
            FP frictionA = FP.Max(bodyA.Friction, FP.Zero);
            FP frictionB = FP.Max(bodyB.Friction, FP.Zero);
            FP product = frictionA * frictionB;
            return product > FP.Zero ? FP.Sqrt(product) : FP.Zero;
        }

        private static FPVector3 ComputeTangent(FPVector3 normal)
        {
            FP absX = FP.Abs(normal.x);
            FP absY = FP.Abs(normal.y);
            FP absZ = FP.Abs(normal.z);

            FPVector3 reference;
            if (absX <= absY && absX <= absZ)
                reference = FPVector3.Right;
            else if (absY <= absZ)
                reference = FPVector3.Up;
            else
                reference = FPVector3.Forward;

            FPVector3 tangent = FPVector3.Cross(normal, reference);
            FP magnitude = tangent.Magnitude;
            if (magnitude <= TinyEpsilon)
            {
                tangent = FPVector3.Cross(normal, FPVector3.Up);
                magnitude = tangent.Magnitude;
            }

            if (magnitude <= TinyEpsilon)
                return FPVector3.Right;
            return tangent / magnitude;
        }
    }
}
