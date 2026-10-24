//-----------------------------------------------------------------------------
// CYGOrientedContacts.cs
// Small allocation-free contact set used by capsule-box sampling.
// Zero UnityEngine dependencies. All math is Q32.32 fixed-point.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Small allocation-free contact set used by capsule-box sampling.
    /// </summary>
    internal struct CYGOrientedContacts
    {
        public FPVector3 Normal;
        public int Count;
        private FPVector3 _p0;
        private FPVector3 _p1;
        private FPVector3 _p2;
        private FPVector3 _p3;
        private FP _d0;
        private FP _d1;
        private FP _d2;
        private FP _d3;

        public void Add(FPVector3 point, FP depth)
        {
            if (depth <= FP.Zero)
                return;
            if (Count < 4)
            {
                Set(Count, point, depth);
                Count++;
                return;
            }

            int shallowest = 0;
            FP min = _d0;
            if (_d1 < min) { shallowest = 1; min = _d1; }
            if (_d2 < min) { shallowest = 2; min = _d2; }
            if (_d3 < min) { shallowest = 3; }
            if (depth > min)
                Set(shallowest, point, depth);
        }

        public FPVector3 GetPoint(int index)
        {
            switch (index)
            {
                case 0: return _p0;
                case 1: return _p1;
                case 2: return _p2;
                default: return _p3;
            }
        }

        public FP GetDepth(int index)
        {
            switch (index)
            {
                case 0: return _d0;
                case 1: return _d1;
                case 2: return _d2;
                default: return _d3;
            }
        }

        private void Set(int index, FPVector3 point, FP depth)
        {
            switch (index)
            {
                case 0: _p0 = point; _d0 = depth; break;
                case 1: _p1 = point; _d1 = depth; break;
                case 2: _p2 = point; _d2 = depth; break;
                default: _p3 = point; _d3 = depth; break;
            }
        }
    }
}
