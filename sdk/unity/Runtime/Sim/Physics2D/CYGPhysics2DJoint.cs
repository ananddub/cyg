//-----------------------------------------------------------------------------
// CYGPhysics2DJoint.cs
// Rigid distance and soft spring joints for the deterministic 2D world.
// Zero UnityEngine dependencies. All math is Q32.32 fixed-point.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    /// <summary>Type of joint solved by <see cref="CYGPhysics2DWorld"/>.</summary>
    public enum CYGPhysics2DJointType : byte
    {
        /// <summary>Rigid constraint keeping the anchor distance at RestLength.</summary>
        Distance = 0,

        /// <summary>Soft spring with stiffness and damping.</summary>
        Spring = 1
    }

    /// <summary>
    /// Two-body joint with local-space anchors. Anchors rotate with their body;
    /// a static body therefore acts as a fixed world anchor. Body ids are stored
    /// in the order supplied by the caller; joint impulses are bilateral.
    /// </summary>
    [Serializable]
    public struct CYGPhysics2DJoint
    {
        public int Id;
        public bool Active;
        public CYGPhysics2DJointType Type;
        public int BodyA;
        public int BodyB;

        /// <summary>Anchor in body A local space.</summary>
        public FPVector2 LocalAnchorA;

        /// <summary>Anchor in body B local space.</summary>
        public FPVector2 LocalAnchorB;

        /// <summary>Target distance between the two world anchors.</summary>
        public FP RestLength;

        /// <summary>Spring stiffness in force per unit length. Unused by distance joints.</summary>
        public FP Stiffness;

        /// <summary>Spring damping in force per unit velocity. Unused by distance joints.</summary>
        public FP Damping;
    }
}
