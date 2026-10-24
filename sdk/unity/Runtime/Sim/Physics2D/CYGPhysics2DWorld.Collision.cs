//-----------------------------------------------------------------------------
// CYGPhysics2DWorld.Collision.cs
// Sequential impulse solver, positional correction, joints and wake rules.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    public sealed partial class CYGPhysics2DWorld
    {
        private struct Contact
        {
            public int SlotA;
            public int SlotB;
            public FPVector2 Normal;
            public FP Friction;
            public FP Restitution;
            public FP RestitutionTarget;
            public int PointCount;
            public FPVector2 Point0;
            public FPVector2 Point1;
            public FP NormalImpulse0;
            public FP TangentImpulse0;
            public FP NormalImpulse1;
            public FP TangentImpulse1;
        }

        private Contact[] _contacts;
        private int _contactCount;

        private void BuildContacts()
        {
            _contactCount = 0;
            _stepCollisionCount = 0;
            _stepTriggerCount = 0;

            for (int i = 0; i < _pairCount; i++)
            {
                CYGPhysics2DPair pair = _pairs[i];
                int slotA = FindSlot(pair.BodyA);
                int slotB = FindSlot(pair.BodyB);
                if (slotA < 0 || slotB < 0)
                {
                    continue;
                }

                ref CYGPhysics2DBody bodyA = ref _bodies[slotA];
                ref CYGPhysics2DBody bodyB = ref _bodies[slotB];

                CYGPhysics2DManifold manifold;
                if (!CYGPhysics2DNarrowphase.Collide(bodyA, bodyB, out manifold))
                {
                    continue;
                }

                if (bodyA.IsTrigger || bodyB.IsTrigger)
                {
                    _stepTriggerPairs[_stepTriggerCount++] = pair;
                    continue;
                }

                _stepCollisionPairs[_stepCollisionCount++] = pair;

                bool sleepingA = IsSleepingDynamic(bodyA);
                bool sleepingB = IsSleepingDynamic(bodyB);

                if (sleepingA && sleepingB)
                {
                    continue;
                }

                if (sleepingA)
                {
                    if (!CanWakeFrom(bodyB))
                    {
                        continue;
                    }

                    WakeSlot(slotA);
                    sleepingA = false;
                }

                if (sleepingB)
                {
                    if (!CanWakeFrom(bodyA))
                    {
                        continue;
                    }

                    WakeSlot(slotB);
                    sleepingB = false;
                }

                if (bodyA.InverseMass <= FP.Zero && bodyB.InverseMass <= FP.Zero)
                {
                    continue;
                }

                Contact contact = new Contact();
                contact.SlotA = slotA;
                contact.SlotB = slotB;
                contact.Normal = manifold.Normal;
                contact.Friction = FP.Sqrt(bodyA.Friction * bodyB.Friction);
                contact.Restitution = FP.Max(bodyA.Restitution, bodyB.Restitution);
                contact.PointCount = manifold.PointCount;
                contact.Point0 = manifold.Point0;
                contact.Point1 = manifold.Point1;

                FPVector2 leverA = manifold.Point0 - bodyA.Position;
                FPVector2 leverB = manifold.Point0 - bodyB.Position;
                FPVector2 relativeVelocity = GetVelocityAtPoint(bodyB, leverB) - GetVelocityAtPoint(bodyA, leverA);
                FP approachingSpeed = FPVector2.Dot(relativeVelocity, manifold.Normal);
                contact.RestitutionTarget = approachingSpeed < -_restitutionThreshold
                    ? -contact.Restitution * approachingSpeed
                    : FP.Zero;

                _contacts[_contactCount++] = contact;
            }
        }

        private static bool IsSleepingDynamic(in CYGPhysics2DBody body)
        {
            return body.IsDynamic && !body.IsAwake;
        }

        private static bool CanWakeFrom(in CYGPhysics2DBody other)
        {
            return other.Type == CYGPhysics2DBodyType.Kinematic
                || (other.IsDynamic && other.IsAwake);
        }

        private static FPVector2 GetVelocityAtPoint(in CYGPhysics2DBody body, FPVector2 lever)
        {
            return body.Velocity + CYGPhysics2DMath.Cross(body.AngularVelocity, lever);
        }

        private void SolveVelocities()
        {
            for (int iteration = 0; iteration < _velocityIterations; iteration++)
            {
                for (int i = 0; i < _contactCount; i++)
                {
                    ref Contact contact = ref _contacts[i];
                    ref CYGPhysics2DBody bodyA = ref _bodies[contact.SlotA];
                    ref CYGPhysics2DBody bodyB = ref _bodies[contact.SlotB];

                    SolveContact(
                        ref bodyA,
                        ref bodyB,
                        contact.Normal,
                        contact.Friction,
                        contact.RestitutionTarget,
                        contact.Point0,
                        contact.Point1,
                        contact.PointCount,
                        ref contact.NormalImpulse0,
                        ref contact.NormalImpulse1,
                        ref contact.TangentImpulse0,
                        ref contact.TangentImpulse1);
                }

                for (int i = 0; i < _jointCount; i++)
                {
                    ref CYGPhysics2DJoint joint = ref _joints[i];
                    if (!joint.Active || joint.Type != CYGPhysics2DJointType.Distance)
                    {
                        continue;
                    }

                    int slotA = FindSlot(joint.BodyA);
                    int slotB = FindSlot(joint.BodyB);
                    if (slotA < 0 || slotB < 0)
                    {
                        continue;
                    }

                    ref CYGPhysics2DBody bodyA = ref _bodies[slotA];
                    ref CYGPhysics2DBody bodyB = ref _bodies[slotB];
                    SolveDistanceJointVelocity(ref bodyA, ref bodyB, joint.LocalAnchorA, joint.LocalAnchorB);
                }
            }
        }

        private static void SolveContact(
            ref CYGPhysics2DBody bodyA,
            ref CYGPhysics2DBody bodyB,
            FPVector2 normal,
            FP friction,
            FP restitutionTarget,
            FPVector2 point0,
            FPVector2 point1,
            int pointCount,
            ref FP accumulatedNormal0,
            ref FP accumulatedNormal1,
            ref FP accumulatedTangent0,
            ref FP accumulatedTangent1)
        {
            if (pointCount <= 1)
            {
                SolveContactPoint(
                    ref bodyA,
                    ref bodyB,
                    normal,
                    friction,
                    restitutionTarget,
                    point0,
                    ref accumulatedNormal0,
                    ref accumulatedTangent0);
                return;
            }

            // Two-point manifolds are solved as a 2x2 normal block. Solving the
            // points sequentially biases stacked boxes toward rotation because the
            // first point changes the angular velocity seen by the second one.
            SolveNormalBlock(
                ref bodyA,
                ref bodyB,
                normal,
                restitutionTarget,
                point0,
                point1,
                ref accumulatedNormal0,
                ref accumulatedNormal1);

            SolveFrictionPoint(
                ref bodyA,
                ref bodyB,
                normal,
                friction,
                point0,
                accumulatedNormal0,
                ref accumulatedTangent0);

            SolveFrictionPoint(
                ref bodyA,
                ref bodyB,
                normal,
                friction,
                point1,
                accumulatedNormal1,
                ref accumulatedTangent1);
        }

        private static void SolveNormalBlock(
            ref CYGPhysics2DBody bodyA,
            ref CYGPhysics2DBody bodyB,
            FPVector2 normal,
            FP restitutionTarget,
            FPVector2 point0,
            FPVector2 point1,
            ref FP accumulated0,
            ref FP accumulated1)
        {
            FP inverseMassSum = bodyA.InverseMass + bodyB.InverseMass;
            if (inverseMassSum <= FP.Zero)
            {
                return;
            }

            FPVector2 leverA0 = point0 - bodyA.Position;
            FPVector2 leverB0 = point0 - bodyB.Position;
            FPVector2 leverA1 = point1 - bodyA.Position;
            FPVector2 leverB1 = point1 - bodyB.Position;

            FP crossA0 = CYGPhysics2DMath.Cross(leverA0, normal);
            FP crossB0 = CYGPhysics2DMath.Cross(leverB0, normal);
            FP crossA1 = CYGPhysics2DMath.Cross(leverA1, normal);
            FP crossB1 = CYGPhysics2DMath.Cross(leverB1, normal);

            FP k11 = inverseMassSum
                + bodyA.InverseInertia * crossA0 * crossA0
                + bodyB.InverseInertia * crossB0 * crossB0;
            FP k22 = inverseMassSum
                + bodyA.InverseInertia * crossA1 * crossA1
                + bodyB.InverseInertia * crossB1 * crossB1;
            FP k12 = inverseMassSum
                + bodyA.InverseInertia * crossA0 * crossA1
                + bodyB.InverseInertia * crossB0 * crossB1;
            FP determinant = k11 * k22 - k12 * k12;

            if (determinant <= FP.Zero || k11 <= FP.Zero || k22 <= FP.Zero)
            {
                // Degenerate block (for example coincident contact points):
                // fall back to scalar normal solves rather than dropping support.
                SolveNormalPoint(ref bodyA, ref bodyB, normal, restitutionTarget, point0, ref accumulated0);
                SolveNormalPoint(ref bodyA, ref bodyB, normal, restitutionTarget, point1, ref accumulated1);
                return;
            }

            FPVector2 velocityA0 = GetVelocityAtPoint(bodyA, leverA0);
            FPVector2 velocityB0 = GetVelocityAtPoint(bodyB, leverB0);
            FPVector2 velocityA1 = GetVelocityAtPoint(bodyA, leverA1);
            FPVector2 velocityB1 = GetVelocityAtPoint(bodyB, leverB1);
            FP normalVelocity0 = FPVector2.Dot(velocityB0 - velocityA0, normal);
            FP normalVelocity1 = FPVector2.Dot(velocityB1 - velocityA1, normal);
            FP target0 = restitutionTarget - normalVelocity0;
            FP target1 = restitutionTarget - normalVelocity1;

            FP lambda0 = (k22 * target0 - k12 * target1) / determinant;
            FP lambda1 = (k11 * target1 - k12 * target0) / determinant;

            FP new0 = accumulated0 + lambda0;
            FP new1 = accumulated1 + lambda1;
            FP delta0;
            FP delta1;

            if (new0 >= FP.Zero && new1 >= FP.Zero)
            {
                delta0 = lambda0;
                delta1 = lambda1;
                accumulated0 = new0;
                accumulated1 = new1;
            }
            else if (new0 < FP.Zero && new1 < FP.Zero)
            {
                delta0 = -accumulated0;
                delta1 = -accumulated1;
                accumulated0 = FP.Zero;
                accumulated1 = FP.Zero;
            }
            else if (new0 < FP.Zero)
            {
                // Point 0 is clamped to zero; solve point 1 with its own mass.
                delta0 = -accumulated0;
                accumulated0 = FP.Zero;
                FP target = (target1 - k12 * delta0) / k22;
                FP candidate = accumulated1 + target;
                if (candidate >= FP.Zero)
                {
                    delta1 = target;
                    accumulated1 = candidate;
                }
                else
                {
                    delta1 = -accumulated1;
                    accumulated1 = FP.Zero;
                }
            }
            else
            {
                delta1 = -accumulated1;
                accumulated1 = FP.Zero;
                FP target = (target0 - k12 * delta1) / k11;
                FP candidate = accumulated0 + target;
                if (candidate >= FP.Zero)
                {
                    delta0 = target;
                    accumulated0 = candidate;
                }
                else
                {
                    delta0 = -accumulated0;
                    accumulated0 = FP.Zero;
                }
            }

            ApplyContactImpulse(ref bodyA, ref bodyB, normal * delta0, leverA0, leverB0);
            ApplyContactImpulse(ref bodyA, ref bodyB, normal * delta1, leverA1, leverB1);
        }

        private static void SolveFrictionPoint(
            ref CYGPhysics2DBody bodyA,
            ref CYGPhysics2DBody bodyB,
            FPVector2 normal,
            FP friction,
            FPVector2 point,
            FP accumulatedNormal,
            ref FP accumulatedTangent)
        {
            FP inverseMassSum = bodyA.InverseMass + bodyB.InverseMass;
            if (inverseMassSum <= FP.Zero)
            {
                return;
            }

            FPVector2 leverA = point - bodyA.Position;
            FPVector2 leverB = point - bodyB.Position;
            FPVector2 tangent = FPVector2.Perpendicular(normal);
            FP tangentCrossA = CYGPhysics2DMath.Cross(leverA, tangent);
            FP tangentCrossB = CYGPhysics2DMath.Cross(leverB, tangent);
            FP tangentMass = inverseMassSum
                + bodyA.InverseInertia * tangentCrossA * tangentCrossA
                + bodyB.InverseInertia * tangentCrossB * tangentCrossB;

            if (tangentMass <= FP.Zero)
            {
                return;
            }

            FPVector2 relativeVelocity = GetVelocityAtPoint(bodyB, leverB) - GetVelocityAtPoint(bodyA, leverA);
            FP tangentVelocity = FPVector2.Dot(relativeVelocity, tangent);
            FP tangentLambda = -tangentVelocity / tangentMass;
            FP maxFriction = friction * accumulatedNormal;

            FP newTangent = FP.Clamp(accumulatedTangent + tangentLambda, -maxFriction, maxFriction);
            FP delta = newTangent - accumulatedTangent;
            accumulatedTangent = newTangent;
            ApplyContactImpulse(ref bodyA, ref bodyB, tangent * delta, leverA, leverB);
        }

        private static void SolveContactPoint(
            ref CYGPhysics2DBody bodyA,
            ref CYGPhysics2DBody bodyB,
            FPVector2 normal,
            FP friction,
            FP restitutionTarget,
            FPVector2 point,
            ref FP accumulatedNormal,
            ref FP accumulatedTangent)
        {
            SolveNormalPoint(ref bodyA, ref bodyB, normal, restitutionTarget, point, ref accumulatedNormal);
            SolveFrictionPoint(
                ref bodyA,
                ref bodyB,
                normal,
                friction,
                point,
                accumulatedNormal,
                ref accumulatedTangent);
        }

        private static void SolveNormalPoint(
            ref CYGPhysics2DBody bodyA,
            ref CYGPhysics2DBody bodyB,
            FPVector2 normal,
            FP restitutionTarget,
            FPVector2 point,
            ref FP accumulatedNormal)
        {
            FP inverseMassSum = bodyA.InverseMass + bodyB.InverseMass;
            if (inverseMassSum <= FP.Zero)
            {
                return;
            }

            FPVector2 leverA = point - bodyA.Position;
            FPVector2 leverB = point - bodyB.Position;
            FP normalCrossA = CYGPhysics2DMath.Cross(leverA, normal);
            FP normalCrossB = CYGPhysics2DMath.Cross(leverB, normal);
            FP normalMass = inverseMassSum
                + bodyA.InverseInertia * normalCrossA * normalCrossA
                + bodyB.InverseInertia * normalCrossB * normalCrossB;

            if (normalMass <= FP.Zero)
            {
                return;
            }

            FPVector2 relativeVelocity = GetVelocityAtPoint(bodyB, leverB) - GetVelocityAtPoint(bodyA, leverA);
            FP normalVelocity = FPVector2.Dot(relativeVelocity, normal);
            FP normalLambda = (restitutionTarget - normalVelocity) / normalMass;

            FP oldNormal = accumulatedNormal;
            FP newNormal = FP.Max(oldNormal + normalLambda, FP.Zero);
            accumulatedNormal = newNormal;
            ApplyContactImpulse(ref bodyA, ref bodyB, normal * (newNormal - oldNormal), leverA, leverB);
        }

        private static void ApplyContactImpulse(
            ref CYGPhysics2DBody bodyA,
            ref CYGPhysics2DBody bodyB,
            FPVector2 impulse,
            FPVector2 leverA,
            FPVector2 leverB)
        {
            bodyA.Velocity -= impulse * bodyA.InverseMass;
            bodyA.AngularVelocity -= bodyA.InverseInertia * CYGPhysics2DMath.Cross(leverA, impulse);
            bodyB.Velocity += impulse * bodyB.InverseMass;
            bodyB.AngularVelocity += bodyB.InverseInertia * CYGPhysics2DMath.Cross(leverB, impulse);
        }

        private void SolvePositions()
        {
            for (int iteration = 0; iteration < _positionIterations; iteration++)
            {
                for (int i = 0; i < _contactCount; i++)
                {
                    Contact contact = _contacts[i];
                    ref CYGPhysics2DBody bodyA = ref _bodies[contact.SlotA];
                    ref CYGPhysics2DBody bodyB = ref _bodies[contact.SlotB];

                    FP inverseMassSum = bodyA.InverseMass + bodyB.InverseMass;
                    if (inverseMassSum <= FP.Zero)
                    {
                        continue;
                    }

                    CYGPhysics2DManifold manifold;
                    if (!CYGPhysics2DNarrowphase.Collide(bodyA, bodyB, out manifold))
                    {
                        continue;
                    }

                    if (manifold.Penetration <= _penetrationSlop)
                    {
                        continue;
                    }

                    FP correction = (manifold.Penetration - _penetrationSlop) * _positionCorrection / inverseMassSum;
                    bodyA.Position -= manifold.Normal * (correction * bodyA.InverseMass);
                    bodyB.Position += manifold.Normal * (correction * bodyB.InverseMass);
                }

                for (int i = 0; i < _jointCount; i++)
                {
                    ref CYGPhysics2DJoint joint = ref _joints[i];
                    if (!joint.Active || joint.Type != CYGPhysics2DJointType.Distance)
                    {
                        continue;
                    }

                    int slotA = FindSlot(joint.BodyA);
                    int slotB = FindSlot(joint.BodyB);
                    if (slotA < 0 || slotB < 0)
                    {
                        continue;
                    }

                    ref CYGPhysics2DBody bodyA = ref _bodies[slotA];
                    ref CYGPhysics2DBody bodyB = ref _bodies[slotB];
                    SolveDistanceJointPosition(
                        ref bodyA,
                        ref bodyB,
                        joint.LocalAnchorA,
                        joint.LocalAnchorB,
                        joint.RestLength);
                }
            }
        }

        private void WakeJointBodies()
        {
            for (int i = 0; i < _jointCount; i++)
            {
                ref CYGPhysics2DJoint joint = ref _joints[i];
                if (!joint.Active)
                {
                    continue;
                }

                int slotA = FindSlot(joint.BodyA);
                int slotB = FindSlot(joint.BodyB);
                if (slotA < 0 || slotB < 0)
                {
                    continue;
                }

                ref CYGPhysics2DBody bodyA = ref _bodies[slotA];
                ref CYGPhysics2DBody bodyB = ref _bodies[slotB];

                if (IsSleepingDynamic(bodyA) && IsActiveMover(bodyB))
                {
                    WakeSlot(slotA);
                }

                if (IsSleepingDynamic(bodyB) && IsActiveMover(bodyA))
                {
                    WakeSlot(slotB);
                }
            }
        }

        private static bool IsActiveMover(in CYGPhysics2DBody body)
        {
            return body.Type == CYGPhysics2DBodyType.Kinematic
                || (body.IsDynamic && body.IsAwake);
        }

        private static bool JointCanSolve(in CYGPhysics2DBody bodyA, in CYGPhysics2DBody bodyB)
        {
            return IsActiveMover(bodyA) || IsActiveMover(bodyB);
        }

        private void ApplySpringJoints()
        {
            FP dt = _fixedDeltaTime;
            for (int i = 0; i < _jointCount; i++)
            {
                ref CYGPhysics2DJoint joint = ref _joints[i];
                if (!joint.Active || joint.Type != CYGPhysics2DJointType.Spring)
                {
                    continue;
                }

                int slotA = FindSlot(joint.BodyA);
                int slotB = FindSlot(joint.BodyB);
                if (slotA < 0 || slotB < 0)
                {
                    continue;
                }

                ref CYGPhysics2DBody bodyA = ref _bodies[slotA];
                ref CYGPhysics2DBody bodyB = ref _bodies[slotB];
                if (!JointCanSolve(bodyA, bodyB))
                {
                    continue;
                }

                FPVector2 leverA = bodyA.RotateVector(joint.LocalAnchorA);
                FPVector2 leverB = bodyB.RotateVector(joint.LocalAnchorB);
                FPVector2 delta = (bodyB.Position + leverB) - (bodyA.Position + leverA);
                FP distance = delta.Magnitude;
                FPVector2 normal = distance > FP.Zero ? delta / distance : FPVector2.Right;

                FP extension = distance - joint.RestLength;
                FPVector2 relativeVelocity = GetVelocityAtPoint(bodyB, leverB) - GetVelocityAtPoint(bodyA, leverA);
                FP normalVelocity = FPVector2.Dot(relativeVelocity, normal);
                FP force = -(joint.Stiffness * extension) - joint.Damping * normalVelocity;
                FP impulseMagnitude = force * dt;

                ApplyContactImpulse(ref bodyA, ref bodyB, normal * impulseMagnitude, leverA, leverB);
            }
        }

        private static void SolveDistanceJointVelocity(
            ref CYGPhysics2DBody bodyA,
            ref CYGPhysics2DBody bodyB,
            FPVector2 localAnchorA,
            FPVector2 localAnchorB)
        {
            if (!JointCanSolve(bodyA, bodyB))
            {
                return;
            }

            FPVector2 leverA = bodyA.RotateVector(localAnchorA);
            FPVector2 leverB = bodyB.RotateVector(localAnchorB);
            FPVector2 delta = (bodyB.Position + leverB) - (bodyA.Position + leverA);
            FP distance = delta.Magnitude;
            FPVector2 normal = distance > FP.Zero ? delta / distance : FPVector2.Right;

            FP inverseMassSum = bodyA.InverseMass + bodyB.InverseMass;
            FP crossA = CYGPhysics2DMath.Cross(leverA, normal);
            FP crossB = CYGPhysics2DMath.Cross(leverB, normal);
            FP effectiveMass = inverseMassSum
                + bodyA.InverseInertia * crossA * crossA
                + bodyB.InverseInertia * crossB * crossB;

            if (effectiveMass <= FP.Zero)
            {
                return;
            }

            FPVector2 relativeVelocity = GetVelocityAtPoint(bodyB, leverB) - GetVelocityAtPoint(bodyA, leverA);
            FP normalVelocity = FPVector2.Dot(relativeVelocity, normal);
            FP lambda = -normalVelocity / effectiveMass;
            ApplyContactImpulse(ref bodyA, ref bodyB, normal * lambda, leverA, leverB);
        }

        private void SolveDistanceJointPosition(
            ref CYGPhysics2DBody bodyA,
            ref CYGPhysics2DBody bodyB,
            FPVector2 localAnchorA,
            FPVector2 localAnchorB,
            FP restLength)
        {
            if (!JointCanSolve(bodyA, bodyB))
            {
                return;
            }

            FPVector2 leverA = bodyA.RotateVector(localAnchorA);
            FPVector2 leverB = bodyB.RotateVector(localAnchorB);
            FPVector2 delta = (bodyB.Position + leverB) - (bodyA.Position + leverA);
            FP distance = delta.Magnitude;
            FPVector2 normal = distance > FP.Zero ? delta / distance : FPVector2.Right;
            FP error = distance - restLength;

            if (FP.Abs(error) <= _penetrationSlop)
            {
                return;
            }

            FP inverseMassSum = bodyA.InverseMass + bodyB.InverseMass;
            FP crossA = CYGPhysics2DMath.Cross(leverA, normal);
            FP crossB = CYGPhysics2DMath.Cross(leverB, normal);
            FP effectiveMass = inverseMassSum
                + bodyA.InverseInertia * crossA * crossA
                + bodyB.InverseInertia * crossB * crossB;

            if (effectiveMass <= FP.Zero)
            {
                return;
            }

            FP lambda = -error * _positionCorrection / effectiveMass;

            bodyA.Position -= normal * (lambda * bodyA.InverseMass);
            bodyA.Angle -= bodyA.InverseInertia * crossA * lambda;
            bodyA.Angle = CYGPhysics2DMath.NormalizeAngle(bodyA.Angle);

            bodyB.Position += normal * (lambda * bodyB.InverseMass);
            bodyB.Angle += bodyB.InverseInertia * crossB * lambda;
            bodyB.Angle = CYGPhysics2DMath.NormalizeAngle(bodyB.Angle);
        }
    }
}
