//-----------------------------------------------------------------------------
// CYGPhysics2DPair.cs
// Stable body-id pair, 2D AABB and raycast result types.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Ordered pair of stable body ids. BodyA is always less than BodyB, which
    /// gives collision and trigger events a deterministic order independent of
    /// broadphase traversal order.
    /// </summary>
    public readonly struct CYGPhysics2DPair : IEquatable<CYGPhysics2DPair>, IComparable<CYGPhysics2DPair>
    {
        public readonly int BodyA;
        public readonly int BodyB;

        public CYGPhysics2DPair(int bodyA, int bodyB)
        {
            if (bodyA <= bodyB)
            {
                BodyA = bodyA;
                BodyB = bodyB;
            }
            else
            {
                BodyA = bodyB;
                BodyB = bodyA;
            }
        }

        public int CompareTo(CYGPhysics2DPair other)
        {
            int byA = BodyA.CompareTo(other.BodyA);
            return byA != 0 ? byA : BodyB.CompareTo(other.BodyB);
        }

        public bool Equals(CYGPhysics2DPair other)
        {
            return BodyA == other.BodyA && BodyB == other.BodyB;
        }

        public override bool Equals(object obj)
        {
            return obj is CYGPhysics2DPair other && Equals(other);
        }

        public override int GetHashCode()
        {
            return BodyA ^ (BodyB * 397);
        }

        public override string ToString()
        {
            return "(" + BodyA + ", " + BodyB + ")";
        }
    }

    /// <summary>Axis-aligned bounds with fixed-point coordinates.</summary>
    [Serializable]
    public struct CYGPhysics2DAABB
    {
        public FPVector2 Min;
        public FPVector2 Max;

        public CYGPhysics2DAABB(FPVector2 min, FPVector2 max)
        {
            Min = min;
            Max = max;
        }

        public FPVector2 Center
        {
            get { return (Min + Max) * FP.Half; }
        }

        public FPVector2 Extents
        {
            get { return (Max - Min) * FP.Half; }
        }

        public bool Intersects(CYGPhysics2DAABB other)
        {
            return Max.x >= other.Min.x && Min.x <= other.Max.x
                && Max.y >= other.Min.y && Min.y <= other.Max.y;
        }
    }

    /// <summary>Result of a world raycast. Distance is measured along the normalized direction.</summary>
    public struct CYGPhysics2DRaycastHit
    {
        public bool Hit;
        public int BodyId;
        public FP Distance;
        public FPVector2 Point;
        public FPVector2 Normal;

        public static CYGPhysics2DRaycastHit None
        {
            get { return new CYGPhysics2DRaycastHit(); }
        }
    }
}
