//-----------------------------------------------------------------------------
// CYGPhysicsJoints.cs
// Deterministic distance and spring joints for the CYG 3D rigid body MVP.
// Zero UnityEngine dependencies. All math is Q32.32 fixed-point.
//-----------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Constraint type solved by <see cref="CYGPhysicsWorld"/>.
    /// </summary>
    public enum CYGJointType
    {
        /// <summary>Rigid distance constraint solved with velocity and position passes.</summary>
        Distance = 0,

        /// <summary>Soft Hooke spring with stiffness and damping.</summary>
        Spring = 1
    }

    /// <summary>
    /// Joint between two bodies. Anchors are local-space offsets rotated by the
    /// respective body orientation; static bodies act as world anchors.
    /// </summary>
    /// <remarks>
    /// Distance and spring joints apply linear and angular impulses at their
    /// world-space anchors, so off-centre anchors rotate dynamic bodies. Position
    /// correction remains translational; there are no hinge or 6-DOF constraints.
    /// </remarks>
    [Serializable]
    public sealed class CYGJoint
    {
        public int Id { get; internal set; } = -1;

        public CYGJointType Type = CYGJointType.Distance;

        public int BodyA;
        public int BodyB;

        /// <summary>Local-space anchor offset on body A.</summary>
        public FPVector3 AnchorA;

        /// <summary>Local-space anchor offset on body B.</summary>
        public FPVector3 AnchorB;

        /// <summary>Target distance between the two world-space anchors.</summary>
        public FP RestLength;

        /// <summary>Spring stiffness in force per unit length. Spring joints only.</summary>
        public FP Stiffness;

        /// <summary>Spring damping in force per unit velocity. Spring joints only.</summary>
        public FP Damping;

        public bool IsActive = true;

        public static CYGJoint CreateDistance(int bodyA, int bodyB, FPVector3 anchorA, FPVector3 anchorB, FP restLength)
        {
            return new CYGJoint
            {
                Type = CYGJointType.Distance,
                BodyA = bodyA,
                BodyB = bodyB,
                AnchorA = anchorA,
                AnchorB = anchorB,
                RestLength = restLength
            };
        }

        public static CYGJoint CreateSpring(int bodyA, int bodyB, FPVector3 anchorA, FPVector3 anchorB, FP restLength, FP stiffness, FP damping)
        {
            return new CYGJoint
            {
                Type = CYGJointType.Spring,
                BodyA = bodyA,
                BodyB = bodyB,
                AnchorA = anchorA,
                AnchorB = anchorB,
                RestLength = restLength,
                Stiffness = stiffness,
                Damping = damping
            };
        }
    }

    internal static class CYGJointMath
    {
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static FPVector3 WorldAnchor(CYGPhysicsBody body, FPVector3 localAnchor)
        {
            if (localAnchor == FPVector3.Zero || body.Rotation == FPQuaternion.Identity)
                return body.Position + localAnchor;
            return body.Position + FPQuaternion.Rotate(body.Rotation, localAnchor);
        }
    }
}
