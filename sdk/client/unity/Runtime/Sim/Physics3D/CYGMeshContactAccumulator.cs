//-----------------------------------------------------------------------------
// CYGMeshContactAccumulator.cs
// Fixed-size, allocation-free collector for mesh contact points.
// Zero UnityEngine dependencies. All math is Q32.32 fixed-point.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Fixed-size, allocation-free collector that keeps the four deepest
    /// contacts and the normal of the deepest one.
    /// </summary>
    internal struct CYGMeshContactAccumulator
    {
        public FPVector3 Normal;
        public int Count;

        private FP _bestDepth;
        private FPVector3 _point0;
        private FPVector3 _point1;
        private FPVector3 _point2;
        private FPVector3 _point3;
        private FP _depth0;
        private FP _depth1;
        private FP _depth2;
        private FP _depth3;

        public void Add(FPVector3 point, FP depth, FPVector3 normal)
        {
            if (depth <= FP.Zero)
                return;

            // Adjacent triangles share edges, so the same box corner can be
            // reported twice. Keeping duplicates would fill the four slots and
            // shift the centre of pressure away from the real contact patch.
            FP duplicateTolerance = FP.FromFraction(1, 1000);
            FP duplicateSquared = duplicateTolerance * duplicateTolerance;
            for (int i = 0; i < Count; i++)
            {
                if ((GetPoint(i) - point).SqrMagnitude > duplicateSquared)
                    continue;
                if (depth <= GetDepth(i))
                    return;
                Set(i, point, depth);
                if (depth > _bestDepth)
                {
                    _bestDepth = depth;
                    Normal = normal;
                }
                return;
            }

            if (depth > _bestDepth)
            {
                _bestDepth = depth;
                Normal = normal;
            }

            if (Count < 4)
            {
                Set(Count, point, depth);
                Count++;
                return;
            }

            int shallowest = 0;
            FP shallowestDepth = _depth0;
            if (_depth1 < shallowestDepth) { shallowest = 1; shallowestDepth = _depth1; }
            if (_depth2 < shallowestDepth) { shallowest = 2; shallowestDepth = _depth2; }
            if (_depth3 < shallowestDepth) { shallowest = 3; shallowestDepth = _depth3; }
            if (depth > shallowestDepth)
                Set(shallowest, point, depth);
        }

        public FPVector3 GetPoint(int index)
        {
            switch (index)
            {
                case 0: return _point0;
                case 1: return _point1;
                case 2: return _point2;
                default: return _point3;
            }
        }

        public FP GetDepth(int index)
        {
            switch (index)
            {
                case 0: return _depth0;
                case 1: return _depth1;
                case 2: return _depth2;
                default: return _depth3;
            }
        }

        private void Set(int index, FPVector3 point, FP depth)
        {
            switch (index)
            {
                case 0: _point0 = point; _depth0 = depth; break;
                case 1: _point1 = point; _depth1 = depth; break;
                case 2: _point2 = point; _depth2 = depth; break;
                default: _point3 = point; _depth3 = depth; break;
            }
        }
    }
}
