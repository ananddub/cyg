//-----------------------------------------------------------------------------
// CYGPathfinder.cs
// Deterministic A* over baked navmesh triangles plus simple stupid funnel
// string pulling. Zero UnityEngine dependencies, all math is Q32.32 fixed point.
//
// Determinism:
//  - Neighbours are visited in the sorted order produced by the builder.
//  - Off-mesh links are visited in ascending authored link index.
//  - The binary heap orders by (f, g, triangle index), so equal-cost routes
//    always resolve to the same path.
//  - No dictionaries or hash sets are enumerated during a query.
//
// LIMITATIONS (MVP):
//  - Static mesh only: dynamic obstacles must be represented by updating the
//    query filter / region mask before each query or by rebaking.
//  - Off-mesh link traversal is planned as a waypoint jump. Agents walk the
//    straight line between the link exit and the next waypoint; animation of
//    the link itself is left to the game layer.
//  - Heuristic uses centroid distance and assumes costs near one. It is
//    deterministic but not guaranteed admissible for very small costs.
//-----------------------------------------------------------------------------

using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Reusable deterministic triangle A* and funnel pathfinder. A single
    /// instance owns scratch buffers and is not thread safe; create one per
    /// simulation world. Query results are freshly allocated and owned by the
    /// caller.
    /// </summary>
    public sealed class CYGPathfinder
    {
        private const int FormatVersion = 1;
        private static readonly FP LinkCostScale = FP.One;

        private struct HeapEntry
        {
            public int Node;
            public FP G;
            public FP F;

            public HeapEntry(int node, FP g, FP f)
            {
                Node = node;
                G = g;
                F = f;
            }
        }

        private struct Portal
        {
            public FPVector3 Left;
            public FPVector3 Right;

            public Portal(FPVector3 left, FPVector3 right)
            {
                Left = left;
                Right = right;
            }
        }

        private readonly CYGNavMesh _mesh;
        private readonly FP[] _gScores;
        private readonly int[] _cameFrom;
        private readonly int[] _cameFromLink;
        private readonly int[] _visitStamp;
        private readonly int[] _pathScratch;
        private readonly int[] _linkOffsets;
        private readonly int[] _linkIndices;
        private readonly FP[] _regionCosts;
        private readonly List<Portal> _portals;
        private readonly List<FPVector3> _waypoints;

        private HeapEntry[] _heap;
        private int _heapCount;
        private int _queryStamp;
        private FP _heuristicWeight = FP.One;
        private int _maxNodeExpansions;

        public CYGPathfinder(CYGNavMesh mesh)
        {
            if (mesh == null)
                throw new ArgumentNullException(nameof(mesh));

            _mesh = mesh;
            int triangleCount = mesh.TriangleCount;

            _gScores = new FP[triangleCount];
            _cameFrom = new int[triangleCount];
            _cameFromLink = new int[triangleCount];
            _visitStamp = new int[triangleCount];
            _pathScratch = new int[triangleCount];
            _regionCosts = new FP[32];
            _portals = new List<Portal>(triangleCount + 4);
            _waypoints = new List<FPVector3>(triangleCount + 4);
            _heap = new HeapEntry[Math.Max(16, triangleCount * 4)];

            for (int i = 0; i < _regionCosts.Length; i++)
                _regionCosts[i] = FP.One;

            BuildLinkAdjacency(mesh, out _linkOffsets, out _linkIndices);
        }

        public CYGNavMesh Mesh
        {
            get { return _mesh; }
        }

        /// <summary>Multiplier applied to the centroid-distance heuristic. 1 is normal, 0 is Dijkstra.</summary>
        public FP HeuristicWeight
        {
            get { return _heuristicWeight; }
            set
            {
                if (value < FP.Zero)
                    throw new ArgumentOutOfRangeException(nameof(value), "Heuristic weight must be non-negative.");
                _heuristicWeight = value;
            }
        }

        /// <summary>Optional safety limit. Zero means unlimited.</summary>
        public int MaxNodeExpansions
        {
            get { return _maxNodeExpansions; }
            set
            {
                if (value < 0)
                    throw new ArgumentOutOfRangeException(nameof(value));
                _maxNodeExpansions = value;
            }
        }

        /// <summary>Sets a multiplier for all triangles in a region. Default is one.</summary>
        public void SetRegionCost(int regionId, FP cost)
        {
            if (regionId < 0 || regionId >= 32)
                throw new ArgumentOutOfRangeException(nameof(regionId), "Region id must be in [0, 31].");
            if (cost < FP.Zero)
                throw new ArgumentOutOfRangeException(nameof(cost), "Region cost must be non-negative.");

            _regionCosts[regionId] = cost;
        }

        public void ResetRegionCosts()
        {
            for (int i = 0; i < _regionCosts.Length; i++)
                _regionCosts[i] = FP.One;
        }

        public CYGPathResult FindPath(FPVector3 start, FPVector3 end)
        {
            return FindPath(start, end, CYGNavQueryFilter.All, FP.Zero);
        }

        public CYGPathResult FindPath(FPVector3 start, FPVector3 end, CYGNavQueryFilter filter)
        {
            return FindPath(start, end, filter, FP.Zero);
        }

        /// <summary>
        /// Finds a path. Points outside the mesh are accepted when they can be
        /// snapped within <paramref name="snapDistance"/>; the snapped points
        /// are then used as the funnel start/end.
        /// </summary>
        public CYGPathResult FindPath(
            FPVector3 start,
            FPVector3 end,
            CYGNavQueryFilter filter,
            FP snapDistance)
        {
            if (_mesh.IsEmpty)
                return CYGPathResult.Failure(CYGPathStatus.InvalidMesh);
            if (snapDistance < FP.Zero)
                throw new ArgumentOutOfRangeException(nameof(snapDistance));

            BeginQuery();

            FPVector3 startPoint;
            int startTriangle;
            if (!ResolvePoint(start, snapDistance, out startPoint, out startTriangle))
                return CYGPathResult.Failure(CYGPathStatus.StartOutsideNavMesh);

            FPVector3 endPoint;
            int endTriangle;
            if (!ResolvePoint(end, snapDistance, out endPoint, out endTriangle))
                return CYGPathResult.Failure(CYGPathStatus.EndOutsideNavMesh);

            CYGNavMeshTriangle startData;
            CYGNavMeshTriangle endData;
            _mesh.TryGetTriangle(startTriangle, out startData);
            _mesh.TryGetTriangle(endTriangle, out endData);

            if (!filter.Allows(startData))
                return CYGPathResult.Failure(CYGPathStatus.StartFiltered);
            if (!filter.Allows(endData))
                return CYGPathResult.Failure(CYGPathStatus.EndFiltered);

            if (startTriangle == endTriangle)
                return BuildDirectResult(startPoint, endPoint, startTriangle);

            FPVector3 endCentroid = _mesh.GetCentroid(endTriangle);
            FP startG = FP.Zero;
            _visitStamp[startTriangle] = _queryStamp;
            _gScores[startTriangle] = startG;
            _cameFrom[startTriangle] = -1;
            _cameFromLink[startTriangle] = -1;

            Push(new HeapEntry(startTriangle, startG, Heuristic(startTriangle, endCentroid)));

            bool found = false;
            int expansions = 0;

            while (_heapCount > 0)
            {
                HeapEntry entry = Pop();
                if (_visitStamp[entry.Node] != _queryStamp || entry.G != _gScores[entry.Node])
                    continue;

                if (entry.Node == endTriangle)
                {
                    found = true;
                    break;
                }

                expansions++;
                if (_maxNodeExpansions > 0 && expansions > _maxNodeExpansions)
                    break;

                ExpandNeighbors(entry.Node, filter, endCentroid);
            }

            if (!found)
                return CYGPathResult.Failure(CYGPathStatus.NoPath);

            int[] trianglePath;
            int trianglePathCount;
            ReconstructPath(startTriangle, endTriangle, out trianglePath, out trianglePathCount);

            _waypoints.Clear();
            _portals.Clear();
            FPVector3 segmentStart = startPoint;

            for (int i = 0; i < trianglePathCount - 1; i++)
            {
                int from = trianglePath[i];
                int to = trianglePath[i + 1];
                int linkIndex = _cameFromLink[to];

                if (linkIndex >= 0)
                {
                    CYGOffMeshLink link = _mesh.GetLinkUnchecked(linkIndex);
                    bool forward = link.FromTriangle == from && link.ToTriangle == to;
                    FPVector3 entryPoint = forward ? link.FromPoint : link.ToPoint;
                    FPVector3 exitPoint = forward ? link.ToPoint : link.FromPoint;

                    RunFunnel(segmentStart, entryPoint, _waypoints);
                    AddWaypoint(exitPoint, segmentStart, _waypoints);
                    segmentStart = exitPoint;
                    _portals.Clear();
                }
                else
                {
                    FPVector3 left;
                    FPVector3 right;
                    if (_mesh.TryGetSharedEdge(from, to, out left, out right))
                        _portals.Add(new Portal(left, right));
                }
            }

            RunFunnel(segmentStart, endPoint, _waypoints);

            int waypointCount = _waypoints.Count;
            FPVector3[] waypoints = new FPVector3[waypointCount];
            for (int i = 0; i < waypointCount; i++)
                waypoints[i] = _waypoints[i];

            return new CYGPathResult(
                CYGPathStatus.Success,
                waypoints,
                waypointCount,
                trianglePath,
                trianglePathCount,
                _gScores[endTriangle],
                startTriangle,
                endTriangle);
        }

        private void ExpandNeighbors(int triangle, CYGNavQueryFilter filter, FPVector3 endCentroid)
        {
            FPVector3 centroid = _mesh.GetCentroid(triangle);
            FP g = _gScores[triangle];

            for (int slot = 0; slot < 3; slot++)
            {
                int next = _mesh.GetNeighborUnchecked(triangle, slot);
                if (next < 0 || !FilterAllows(filter, next))
                    continue;

                CYGNavMeshTriangle nextData;
                _mesh.TryGetTriangle(next, out nextData);
                FP distance = FPVector3.Distance(centroid, _mesh.GetCentroid(next));
                FP cost = distance * nextData.Cost * _regionCosts[nextData.RegionId];
                Relax(triangle, next, g, cost, -1, endCentroid);
            }

            int linkStart = _linkOffsets[triangle];
            int linkEnd = _linkOffsets[triangle + 1];
            for (int i = linkStart; i < linkEnd; i++)
            {
                int linkIndex = _linkIndices[i];
                CYGOffMeshLink link = _mesh.GetLinkUnchecked(linkIndex);
                int next = link.FromTriangle == triangle ? link.ToTriangle : link.FromTriangle;
                if (next == triangle || !FilterAllows(filter, next))
                    continue;

                CYGNavMeshTriangle nextData;
                _mesh.TryGetTriangle(next, out nextData);
                FP cost = link.Cost * LinkCostScale * _regionCosts[nextData.RegionId];
                Relax(triangle, next, g, cost, linkIndex, endCentroid);
            }
        }

        private void Relax(
            int from,
            int to,
            FP fromG,
            FP transitionCost,
            int linkIndex,
            FPVector3 endCentroid)
        {
            FP candidate = fromG + transitionCost;
            bool visited = _visitStamp[to] == _queryStamp;
            if (visited && candidate >= _gScores[to])
                return;

            _visitStamp[to] = _queryStamp;
            _gScores[to] = candidate;
            _cameFrom[to] = from;
            _cameFromLink[to] = linkIndex;
            Push(new HeapEntry(to, candidate, candidate + Heuristic(to, endCentroid)));
        }

        private FP Heuristic(int triangle, FPVector3 endCentroid)
        {
            if (_heuristicWeight == FP.Zero)
                return FP.Zero;

            FP distance = FPVector3.Distance(_mesh.GetCentroid(triangle), endCentroid);
            return distance * _heuristicWeight;
        }

        private bool FilterAllows(CYGNavQueryFilter filter, int triangle)
        {
            CYGNavMeshTriangle data;
            return _mesh.TryGetTriangle(triangle, out data) && filter.Allows(data);
        }

        private bool ResolvePoint(FPVector3 point, FP snapDistance, out FPVector3 resolved, out int triangle)
        {
            if (_mesh.TryFindTriangle(point, out triangle))
            {
                resolved = point;
                return true;
            }

            resolved = point;
            if (snapDistance > FP.Zero)
            {
                FPVector3 snapped;
                int snappedTriangle;
                if (_mesh.SnapToNavMesh(point, snapDistance, out snapped, out snappedTriangle))
                {
                    resolved = snapped;
                    triangle = snappedTriangle;
                    return true;
                }
            }

            triangle = -1;
            return false;
        }

        private CYGPathResult BuildDirectResult(FPVector3 start, FPVector3 end, int triangle)
        {
            if (start == end)
            {
                return new CYGPathResult(
                    CYGPathStatus.Success,
                    new FPVector3[0],
                    0,
                    new int[] { triangle },
                    1,
                    FP.Zero,
                    triangle,
                    triangle);
            }

            return new CYGPathResult(
                CYGPathStatus.Success,
                new FPVector3[] { end },
                1,
                new int[] { triangle },
                1,
                FP.Zero,
                triangle,
                triangle);
        }

        private void ReconstructPath(
            int startTriangle,
            int endTriangle,
            out int[] trianglePath,
            out int trianglePathCount)
        {
            int count = 0;
            int node = endTriangle;
            while (node >= 0 && count < _pathScratch.Length)
            {
                _pathScratch[count++] = node;
                if (node == startTriangle)
                    break;
                node = _cameFrom[node];
            }

            // The chain must terminate at the start triangle.
            if (count == 0 || _pathScratch[count - 1] != startTriangle)
            {
                trianglePath = new int[0];
                trianglePathCount = 0;
                return;
            }

            trianglePath = new int[count];
            for (int i = 0; i < count; i++)
                trianglePath[i] = _pathScratch[count - 1 - i];
            trianglePathCount = count;
        }

        private void RunFunnel(FPVector3 start, FPVector3 end, List<FPVector3> output)
        {
            FPVector3 apex = start;
            FPVector3 left = start;
            FPVector3 right = start;
            int apexIndex = -1;
            int leftIndex = -1;
            int rightIndex = -1;

            int i = 0;
            while (i < _portals.Count)
            {
                FPVector3 newLeft = _portals[i].Left;
                FPVector3 newRight = _portals[i].Right;

                // Tighten the right side of the funnel.
                if (TriArea2(apex, right, newRight) <= FP.Zero)
                {
                    if (apex == right || TriArea2(apex, left, newRight) > FP.Zero)
                    {
                        right = newRight;
                        rightIndex = i;
                    }
                    else
                    {
                        AddWaypoint(left, start, output);
                        apex = left;
                        apexIndex = leftIndex;
                        left = apex;
                        right = apex;
                        leftIndex = apexIndex;
                        rightIndex = apexIndex;
                        i = apexIndex + 1;
                        continue;
                    }
                }

                // Tighten the left side of the funnel.
                if (TriArea2(apex, left, newLeft) >= FP.Zero)
                {
                    if (apex == left || TriArea2(apex, right, newLeft) < FP.Zero)
                    {
                        left = newLeft;
                        leftIndex = i;
                    }
                    else
                    {
                        AddWaypoint(right, start, output);
                        apex = right;
                        apexIndex = rightIndex;
                        left = apex;
                        right = apex;
                        leftIndex = apexIndex;
                        rightIndex = apexIndex;
                        i = apexIndex + 1;
                        continue;
                    }
                }

                i++;
            }

            AddWaypoint(end, start, output);
        }

        private static void AddWaypoint(FPVector3 point, FPVector3 segmentStart, List<FPVector3> output)
        {
            if (point == segmentStart)
                return;
            if (output.Count > 0 && output[output.Count - 1] == point)
                return;

            output.Add(point);
        }

        private static FP TriArea2(FPVector3 a, FPVector3 b, FPVector3 c)
        {
            return ((b.x - a.x) * (c.z - a.z)) - ((c.x - a.x) * (b.z - a.z));
        }

        private void BeginQuery()
        {
            _queryStamp++;
            if (_queryStamp == 0)
            {
                Array.Clear(_visitStamp, 0, _visitStamp.Length);
                _queryStamp = 1;
            }

            _heapCount = 0;
        }

        private static void BuildLinkAdjacency(CYGNavMesh mesh, out int[] offsets, out int[] indices)
        {
            int triangleCount = mesh.TriangleCount;
            int[] counts = new int[triangleCount];
            for (int i = 0; i < mesh.LinkCount; i++)
            {
                CYGOffMeshLink link = mesh.GetLinkUnchecked(i);
                if (link.FromTriangle >= 0 && link.FromTriangle < triangleCount)
                    counts[link.FromTriangle]++;
                if (link.Bidirectional && link.ToTriangle >= 0 && link.ToTriangle < triangleCount)
                    counts[link.ToTriangle]++;
            }

            offsets = new int[triangleCount + 1];
            for (int i = 0; i < triangleCount; i++)
                offsets[i + 1] = offsets[i] + counts[i];

            indices = new int[offsets[triangleCount]];
            int[] cursor = new int[triangleCount];
            Array.Copy(offsets, cursor, triangleCount);

            for (int i = 0; i < mesh.LinkCount; i++)
            {
                CYGOffMeshLink link = mesh.GetLinkUnchecked(i);
                if (link.FromTriangle >= 0 && link.FromTriangle < triangleCount)
                    indices[cursor[link.FromTriangle]++] = i;
                if (link.Bidirectional && link.ToTriangle >= 0 && link.ToTriangle < triangleCount)
                    indices[cursor[link.ToTriangle]++] = i;
            }
        }

        #region Deterministic binary heap

        private void Push(HeapEntry entry)
        {
            if (_heapCount == _heap.Length)
                Array.Resize(ref _heap, _heap.Length * 2);

            int index = _heapCount++;
            _heap[index] = entry;
            while (index > 0)
            {
                int parent = (index - 1) / 2;
                if (!Less(_heap[index], _heap[parent]))
                    break;

                HeapEntry swap = _heap[parent];
                _heap[parent] = _heap[index];
                _heap[index] = swap;
                index = parent;
            }
        }

        private HeapEntry Pop()
        {
            HeapEntry result = _heap[0];
            _heapCount--;
            if (_heapCount > 0)
            {
                _heap[0] = _heap[_heapCount];
                int index = 0;
                while (true)
                {
                    int left = (index * 2) + 1;
                    int right = left + 1;
                    int smallest = index;

                    if (left < _heapCount && Less(_heap[left], _heap[smallest]))
                        smallest = left;
                    if (right < _heapCount && Less(_heap[right], _heap[smallest]))
                        smallest = right;
                    if (smallest == index)
                        break;

                    HeapEntry swap = _heap[index];
                    _heap[index] = _heap[smallest];
                    _heap[smallest] = swap;
                    index = smallest;
                }
            }

            return result;
        }

        private static bool Less(HeapEntry a, HeapEntry b)
        {
            if (a.F != b.F)
                return a.F < b.F;
            if (a.G != b.G)
                return a.G < b.G;
            return a.Node < b.Node;
        }

        #endregion
    }
}
