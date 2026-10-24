//-----------------------------------------------------------------------------
// CYG.Navigation.Tests
// Headless deterministic navigation tests. No UnityEngine references.
//-----------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using CookYourGame.Sim;

namespace CookYourGame.Sim.Tests
{
    internal static class Program
    {
        private static int _assertions;

        private static int Main()
        {
            try
            {
                TestDeterministicBake();
                TestPointQueriesAndSnapping();
                TestPathAcrossOpenGrid();
                TestBlockedRegionDetourAndFailure();
                TestUnreachableIsland();
                TestOffMeshLinkShortcut();
                TestFunnelStraightLine();
                TestAgentFollowsPath();
                TestTwoAgentsCrossing();
                TestDeterminismAndSnapshot();
                TestBuilderValidationAndSlopeLimit();

                Console.WriteLine("CYG navigation tests passed: assertions=" + _assertions);
                return 0;
            }
            catch (Exception exception)
            {
                Console.Error.WriteLine("CYG navigation tests failed: " + exception.Message);
                return 1;
            }
        }

        #region Tests

        private static void TestDeterministicBake()
        {
            GridData grid = BuildGrid(4, 4, 2);
            CYGNavMesh first = CYGNavMeshBuilder.Build(grid.Vertices, grid.Indices);
            CYGNavMesh second = CYGNavMeshBuilder.Build(grid.Vertices, grid.Indices);

            Equal(25, first.VertexCount, "bake vertex count");
            Equal(32, first.TriangleCount, "bake triangle count");
            Equal(0, first.LinkCount, "bake link count");
            Equal(25, second.VertexCount, "second bake vertex count");
            Equal(32, second.TriangleCount, "second bake triangle count");
            Require(first.Checksum == second.Checksum, "two identical bakes must produce equal checksums");
            Require(first.Bounds.Equals(second.Bounds), "two identical bakes must produce equal bounds");

            for (int i = 0; i < first.VertexCount; i++)
                Require(first.GetVertex(i) == second.GetVertex(i), "vertex " + i + " differs between bakes");

            for (int i = 0; i < first.TriangleCount; i++)
            {
                CYGNavMeshTriangle a;
                CYGNavMeshTriangle b;
                first.TryGetTriangle(i, out a);
                second.TryGetTriangle(i, out b);
                Equal(a.Vertex0, b.Vertex0, "triangle " + i + " vertex0");
                Equal(a.Vertex1, b.Vertex1, "triangle " + i + " vertex1");
                Equal(a.Vertex2, b.Vertex2, "triangle " + i + " vertex2");
                Equal(a.RegionId, b.RegionId, "triangle " + i + " region");
                Equal(a.AreaMask, b.AreaMask, "triangle " + i + " area mask");
                Equal(a.Cost, b.Cost, "triangle " + i + " cost");
            }

            for (int triangle = 0; triangle < first.TriangleCount; triangle++)
            {
                for (int slot = 0; slot < 3; slot++)
                    Equal(first.GetNeighbor(triangle, slot), second.GetNeighbor(triangle, slot), "baked adjacency");
            }

            CYGNavMeshTriangle firstTriangle;
            first.TryGetTriangle(0, out firstTriangle);
            Require(firstTriangle.Vertex0 >= 0 && firstTriangle.Vertex1 >= 0 && firstTriangle.Vertex2 >= 0,
                "baked triangle indices must be valid");
            Equal(0, firstTriangle.RegionId, "default triangle region");
            Equal(1u, firstTriangle.AreaMask, "default triangle area mask");
            Equal(FP.One, firstTriangle.Cost, "default triangle cost");
        }

        private static void TestPointQueriesAndSnapping()
        {
            GridData grid = BuildGrid(4, 4, 2);
            CYGNavMesh mesh = CYGNavMeshBuilder.Build(grid.Vertices, grid.Indices);

            FPVector3 inside = V(1, 0, 1);
            int triangle;
            Require(mesh.TryFindTriangle(inside, out triangle), "point (1,0,1) must find a triangle");
            Require(triangle >= 0 && triangle < mesh.TriangleCount, "triangle index must be in range");
            Require(mesh.IsPointOnNavMesh(inside), "point (1,0,1) must be on the navmesh");

            FPVector3 outside = new FPVector3(
                FP.FromFraction(-1, 10),
                FP.Zero,
                FP.FromFraction(-1, 10));
            Require(!mesh.IsPointOnNavMesh(outside), "point (-0.1,0,-0.1) must be off the navmesh");

            FPVector3 snapped;
            int snappedTriangle;
            Require(mesh.SnapToNavMesh(outside, FP.One, out snapped, out snappedTriangle),
                "off-mesh point must snap within one unit");
            Require(snapped == FPVector3.Zero, "corner point must snap to the grid corner");
            Require(snappedTriangle >= 0, "snap must report a triangle");
            Require(!mesh.SnapToNavMesh(outside, FP.FromFraction(1, 20), out snapped),
                "snap must fail when the mesh is farther than the limit");

            FPVector3 above = new FPVector3(FP.FromInt(1), FP.FromFraction(1, 2), FP.FromInt(1));
            Require(!mesh.IsPointOnNavMesh(above, FP.Zero),
                "surface distance zero must reject a point above the plane");
            Require(mesh.IsPointOnNavMesh(above, FP.FromFraction(3, 5)),
                "surface tolerance 0.6 must accept a point 0.5 above the plane");
            Require(!mesh.IsPointOnNavMesh(above, FP.FromFraction(2, 5)),
                "surface tolerance 0.4 must reject a point 0.5 above the plane");

            FPVector3 highAbove = new FPVector3(FP.FromInt(1), FP.FromInt(5), FP.FromInt(1));
            Require(mesh.TryFindTriangle(highAbove, out triangle),
                "XZ projection query must still find a triangle high above the mesh");

            FPVector3 interior = new FPVector3(FP.FromFraction(1, 2), FP.Zero, FP.FromFraction(1, 2));
            Require(!mesh.TryFindTriangle(interior, CYGNavQueryFilter.ForRegion(1), out triangle),
                "region filter must reject triangles outside the selected region");
            Require(mesh.TryFindTriangle(interior, CYGNavQueryFilter.ForRegion(0), out triangle),
                "region filter must accept triangles in the selected region");
            Require(!mesh.IsPointOnNavMesh(interior, CYGNavQueryFilter.All.WithAreaMask(2u)),
                "area filter must reject triangles without a matching area bit");
            Require(mesh.IsPointOnNavMesh(interior, CYGNavQueryFilter.All.WithAreaMask(1u)),
                "area filter must accept triangles with a matching area bit");

            FPVector3 filteredSnap;
            Require(!mesh.SnapToNavMesh(interior, FP.One, CYGNavQueryFilter.ForRegion(1), out filteredSnap),
                "filtered snapping must fail when no allowed triangle is nearby");
        }

        private static void TestPathAcrossOpenGrid()
        {
            GridData grid = BuildGrid(4, 4, 2);
            CYGNavMesh mesh = CYGNavMeshBuilder.Build(grid.Vertices, grid.Indices);
            CYGPathfinder pathfinder = new CYGPathfinder(mesh);

            FPVector3 start = new FPVector3(FP.FromInt(1), FP.Zero, FP.FromFraction(1, 2));
            FPVector3 end = new FPVector3(FP.FromInt(7), FP.Zero, FP.FromFraction(15, 2));

            CYGPathResult first = pathfinder.FindPath(start, end);
            Require(first.Success, "open grid path must succeed");
            Require(first.WaypointCount >= 1 && first.WaypointCount <= 3,
                "open grid path should need only a few waypoints, got " + first.WaypointCount);
            Require(first.GetWaypoint(first.WaypointCount - 1) == end, "last waypoint must be the destination");
            Require(first.TrianglePathCount >= 2, "path must traverse multiple triangles");
            Equal(first.StartTriangle, first.GetTriangle(0), "triangle path must start at the start triangle");
            Equal(first.EndTriangle, first.GetTriangle(first.TrianglePathCount - 1),
                "triangle path must end at the destination triangle");

            FP directDistance = FPVector3.Distance(start, end);
            FP pathLength = PathLength(start, first);
            Require(pathLength <= directDistance + FP.One,
                "open grid path length should be close to the straight line");
            Require(pathLength >= directDistance - FP.FromFraction(1, 100),
                "path length cannot be shorter than the straight line");

            CYGPathResult second = pathfinder.FindPath(start, end);
            AssertSamePath(first, second, "repeated path query");
        }

        private static void TestBlockedRegionDetourAndFailure()
        {
            GridData grid = BuildGrid(4, 4, 2);
            int triangleCount = grid.Indices.Length / 3;

            CYGNavBuildConfig detourConfig = CreateGridConfig(triangleCount);
            SetCellRegion(detourConfig, 4, 1, 1, 2);
            SetCellRegion(detourConfig, 4, 1, 2, 2);
            CYGNavMesh detourMesh = CYGNavMeshBuilder.Build(grid.Vertices, grid.Indices, detourConfig);
            CYGPathfinder detourPathfinder = new CYGPathfinder(detourMesh);

            CYGNavQueryFilter filter = CYGNavQueryFilter.All.WithoutRegion(2);
            FPVector3 start = new FPVector3(FP.FromInt(1), FP.Zero, FP.FromFraction(37, 10));
            FPVector3 end = new FPVector3(FP.FromInt(5), FP.Zero, FP.FromFraction(37, 10));

            CYGPathResult detour = detourPathfinder.FindPath(start, end, filter);
            Require(detour.Success, "a partial barrier must allow a detour");
            AssertPathAvoidsRegion(detourMesh, detour, 2);
            FP straight = FPVector3.Distance(start, end);
            Require(PathLength(start, detour) > straight + FP.One, "detour must be longer than the direct line");

            CYGNavBuildConfig wallConfig = CreateGridConfig(triangleCount);
            for (int z = 0; z < 4; z++)
                SetCellRegion(wallConfig, 4, 1, z, 2);
            CYGNavMesh wallMesh = CYGNavMeshBuilder.Build(grid.Vertices, grid.Indices, wallConfig);
            CYGPathfinder wallPathfinder = new CYGPathfinder(wallMesh);

            CYGPathResult blocked = wallPathfinder.FindPath(start, end, filter);
            Require(!blocked.Success, "a full blocked wall must make the destination unreachable");
            Require(blocked.Status == CYGPathStatus.NoPath, "blocked query must report NoPath");

            CYGPathResult unfiltered = wallPathfinder.FindPath(start, end, CYGNavQueryFilter.All);
            Require(unfiltered.Success, "the same wall must be traversable without the region filter");
        }

        private static void TestUnreachableIsland()
        {
            FPVector3[] vertices = new FPVector3[]
            {
                V(0, 0, 0), V(0, 0, 2), V(2, 0, 0), V(2, 0, 2),
                V(10, 0, 0), V(10, 0, 2), V(12, 0, 0), V(12, 0, 2)
            };
            int[] indices = new int[]
            {
                0, 1, 2,
                2, 1, 3,
                4, 5, 6,
                6, 5, 7
            };

            CYGNavMesh mesh = CYGNavMeshBuilder.Build(vertices, indices);
            Equal(4, mesh.TriangleCount, "island mesh triangle count");
            CYGPathfinder pathfinder = new CYGPathfinder(mesh);

            CYGPathResult result = pathfinder.FindPath(V(1, 0, 1), V(11, 0, 1));
            Require(!result.Success, "separate island must be unreachable");
            Require(result.Status == CYGPathStatus.NoPath, "unreachable island must report NoPath");

            CYGPathResult local = pathfinder.FindPath(V(1, 0, 1), V(1, 0, 1));
            Require(local.Success, "path on one island must still succeed");
        }

        private static void TestOffMeshLinkShortcut()
        {
            GridData grid = BuildGrid(4, 4, 2);
            int triangleCount = grid.Indices.Length / 3;
            FPVector3 linkEntry = new FPVector3(FP.FromFraction(3, 2), FP.Zero, FP.FromFraction(23, 10));
            FPVector3 linkExit = new FPVector3(FP.FromFraction(9, 2), FP.Zero, FP.FromFraction(37, 10));

            CYGNavBuildConfig detourConfig = CreateGridConfig(triangleCount);
            SetCellRegion(detourConfig, 4, 1, 1, 2);
            SetCellRegion(detourConfig, 4, 1, 2, 2);
            CYGNavMesh detourMesh = CYGNavMeshBuilder.Build(grid.Vertices, grid.Indices, detourConfig);

            CYGNavQueryFilter filter = CYGNavQueryFilter.All.WithoutRegion(2);
            CYGPathfinder detourPathfinder = new CYGPathfinder(detourMesh);
            CYGPathResult withoutLink = detourPathfinder.FindPath(linkEntry, linkExit, filter);
            Require(withoutLink.Success, "detour without a link must succeed");

            detourConfig.Links = new CYGOffMeshLink[]
            {
                new CYGOffMeshLink(8, 13, linkEntry, linkExit, FP.One, false)
            };
            CYGNavMesh linkMesh = CYGNavMeshBuilder.Build(grid.Vertices, grid.Indices, detourConfig);
            Equal(1, linkMesh.LinkCount, "baked off-mesh link count");
            CYGPathfinder linkPathfinder = new CYGPathfinder(linkMesh);
            CYGPathResult withLink = linkPathfinder.FindPath(linkEntry, linkExit, filter);

            Require(withLink.Success, "path using an off-mesh link must succeed");
            Require(withLink.PathCost < withoutLink.PathCost, "off-mesh link shortcut must be cheaper than the detour");
            Require(HasConsecutiveTriangles(withLink, 8, 13), "triangle path must use the off-mesh link 8 -> 13");
            Require(ContainsWaypoint(withLink, linkExit), "path must include the link exit point");

            CYGNavBuildConfig wallConfig = CreateGridConfig(triangleCount);
            for (int z = 0; z < 4; z++)
                SetCellRegion(wallConfig, 4, 1, z, 2);
            wallConfig.Links = new CYGOffMeshLink[]
            {
                new CYGOffMeshLink(8, 13, linkEntry, linkExit, FP.One, false)
            };
            CYGNavMesh directedMesh = CYGNavMeshBuilder.Build(grid.Vertices, grid.Indices, wallConfig);
            CYGPathfinder directedPathfinder = new CYGPathfinder(directedMesh);

            CYGPathResult forward = directedPathfinder.FindPath(linkEntry, linkExit, filter);
            Require(forward.Success, "directed off-mesh link must work in the forward direction");
            CYGPathResult reverse = directedPathfinder.FindPath(linkExit, linkEntry, filter);
            Require(!reverse.Success, "directed off-mesh link must not work backwards");

            wallConfig.Links = new CYGOffMeshLink[]
            {
                new CYGOffMeshLink(8, 13, linkEntry, linkExit, FP.One, true)
            };
            CYGNavMesh bidirectionalMesh = CYGNavMeshBuilder.Build(grid.Vertices, grid.Indices, wallConfig);
            CYGPathfinder bidirectionalPathfinder = new CYGPathfinder(bidirectionalMesh);
            CYGPathResult reverseBidirectional = bidirectionalPathfinder.FindPath(linkExit, linkEntry, filter);
            Require(reverseBidirectional.Success, "bidirectional off-mesh link must work backwards");
        }

        private static void TestFunnelStraightLine()
        {
            GridData grid = BuildGrid(4, 4, 2);
            CYGNavMesh mesh = CYGNavMeshBuilder.Build(grid.Vertices, grid.Indices);
            CYGPathfinder pathfinder = new CYGPathfinder(mesh);

            FPVector3 start = V(1, 0, 1);
            FPVector3 end = V(7, 0, 7);
            CYGPathResult result = pathfinder.FindPath(start, end);

            Require(result.Success, "funnel test path must succeed");
            Require(result.WaypointCount >= 1 && result.WaypointCount <= 2,
                "funnel on an open grid must produce at most two waypoints, got " + result.WaypointCount);
            Require(result.GetWaypoint(result.WaypointCount - 1) == end, "funnel destination must be exact");
            Require(PathLength(start, result) <= FPVector3.Distance(start, end) + FP.FromFraction(1, 100),
                "funnel path must be a straight line");
        }

        private static void TestAgentFollowsPath()
        {
            GridData grid = BuildGrid(4, 4, 2);
            CYGNavMesh mesh = CYGNavMeshBuilder.Build(grid.Vertices, grid.Indices);
            CYGAgentSystem system = new CYGAgentSystem(mesh);

            CYGAgent agent = system.AddAgent(
                V(1, 0, 1),
                FP.FromFraction(2, 5),
                FP.FromInt(3),
                FP.FromInt(8));
            FPVector3 destination = V(7, 0, 7);
            agent.SetDestination(destination);

            FP deltaTime = FP.FromFraction(1, 60);
            int ticks = 1200;
            for (int i = 0; i < ticks; i++)
                system.Tick(deltaTime);

            Require(agent.HasReachedDestination, "agent must reach its destination");
            Require(FPVector3.Distance(agent.Position, destination) <= agent.ArrivalTolerance,
                "agent position must be within arrival tolerance");
            Equal(ticks, system.TickCount, "agent system tick count");
            Require(mesh.IsPointOnNavMesh(agent.Position), "agent should remain on the navmesh in this scenario");
            Require(agent.Velocity.Magnitude <= agent.MaxSpeed, "agent velocity must not exceed max speed");
        }

        private static void TestTwoAgentsCrossing()
        {
            GridData grid = BuildGrid(4, 4, 2);
            CYGNavMesh mesh = CYGNavMeshBuilder.Build(grid.Vertices, grid.Indices);
            CYGAgentSystem system = new CYGAgentSystem(mesh);

            FP radius = FP.FromFraction(2, 5);
            FP speed = FP.FromFraction(5, 2);
            FP acceleration = FP.FromInt(8);
            CYGAgent a = system.AddAgent(V(2, 0, 2), radius, speed, acceleration);
            CYGAgent b = system.AddAgent(V(6, 0, 2), radius, speed, acceleration);
            a.SetDestination(V(6, 0, 6));
            b.SetDestination(V(2, 0, 6));

            FP deltaTime = FP.FromFraction(1, 60);
            FP minimumDistance = radius + radius;
            FP closestApproach = FP.MaxValue;
            bool overlapped = false;

            for (int i = 0; i < 1800; i++)
            {
                system.Tick(deltaTime);
                FP distance = HorizontalDistance(a.Position, b.Position);
                if (distance < closestApproach)
                    closestApproach = distance;
                if (distance < minimumDistance - FP.FromFraction(1, 50))
                    overlapped = true;
            }

            Require(!overlapped, "agents must never overlap below the combined radius");
            Require(closestApproach >= minimumDistance - FP.FromFraction(1, 50),
                "closest approach must respect the combined radius");
            Require(closestApproach <= minimumDistance + FP.One, "the crossing test must actually bring agents close");
            Require(a.HasReachedDestination, "first crossing agent must reach its destination");
            Require(b.HasReachedDestination, "second crossing agent must reach its destination");
        }

        private static void TestDeterminismAndSnapshot()
        {
            GridData grid = BuildGrid(4, 4, 2);
            CYGNavMesh mesh = CYGNavMeshBuilder.Build(grid.Vertices, grid.Indices);
            FP deltaTime = FP.FromFraction(1, 60);

            CYGAgentSystem first = CreateCrossingSystem(mesh);
            CYGAgentSystem second = CreateCrossingSystem(mesh);

            for (int step = 0; step < 1000; step++)
            {
                first.Tick(deltaTime);
                second.Tick(deltaTime);

                if (((step + 1) % 100) == 0)
                {
                    Require(first.ComputeChecksum() == second.ComputeChecksum(),
                        "two 1000-step runs diverged at step " + (step + 1));
                }
            }

            Equal(first.ComputeChecksum(), second.ComputeChecksum(), "final checksum after 1000 steps");

            CYGAgentSystem system = CreateCrossingSystem(mesh);
            for (int i = 0; i < 200; i++)
                system.Tick(deltaTime);

            CYGAgentSystemSnapshot snapshot = system.CaptureSnapshot();
            uint capturedChecksum = system.ComputeChecksum();
            Equal(capturedChecksum, snapshot.Checksum, "snapshot checksum must match the live system");
            Equal(3, snapshot.AgentCount, "snapshot agent count");

            for (int i = 0; i < 150; i++)
                system.Tick(deltaTime);
            Require(system.ComputeChecksum() != capturedChecksum, "running after capture must change the checksum");

            Require(system.RestoreSnapshot(snapshot), "snapshot restore must succeed");
            Equal(capturedChecksum, system.ComputeChecksum(), "restored checksum must match the captured checksum");
            Equal(200, system.TickCount, "restored tick count");

            for (int i = 0; i < 300; i++)
                system.Tick(deltaTime);
            uint firstReplay = system.ComputeChecksum();

            Require(system.RestoreSnapshot(snapshot), "second snapshot restore must succeed");
            for (int i = 0; i < 300; i++)
                system.Tick(deltaTime);
            uint secondReplay = system.ComputeChecksum();

            Equal(firstReplay, secondReplay, "replaying from the same snapshot must be deterministic");
            Equal(500, system.TickCount, "replayed tick count");
        }

        private static void TestBuilderValidationAndSlopeLimit()
        {
            FPVector3[] vertices = new FPVector3[]
            {
                V(0, 0, 0), V(2, 0, 0), V(0, 2, 2)
            };

            Throws<ArgumentException>(
                () => CYGNavMeshBuilder.Build(vertices, new int[] { 0, 1, 9 }),
                "out-of-range triangle index must throw");
            Throws<ArgumentException>(
                () => CYGNavMeshBuilder.Build(vertices, new int[] { 0, 1 }),
                "non-multiple-of-three index count must throw");

            CYGNavBuildConfig invalidRegions = CreateGridConfig(1);
            invalidRegions.TriangleRegions[0] = 99;
            Throws<ArgumentException>(
                () => CYGNavMeshBuilder.Build(vertices, new int[] { 0, 1, 2 }, invalidRegions),
                "region id outside [0,31] must throw");

            CYGNavBuildConfig invalidCost = CreateGridConfig(1);
            invalidCost.TriangleCosts[0] = -FP.One;
            Throws<ArgumentException>(
                () => CYGNavMeshBuilder.Build(vertices, new int[] { 0, 1, 2 }, invalidCost),
                "negative triangle cost must throw");

            CYGNavBuildConfig lowSlope = CreateGridConfig(1);
            lowSlope.SlopeLimitDegrees = FP.FromInt(30);
            CYGNavMesh dropped = CYGNavMeshBuilder.Build(vertices, new int[] { 0, 1, 2 }, lowSlope);
            Equal(0, dropped.TriangleCount, "45 degree triangle must be dropped at a 30 degree slope limit");

            CYGNavBuildConfig highSlope = CreateGridConfig(1);
            highSlope.SlopeLimitDegrees = FP.FromInt(60);
            CYGNavMesh kept = CYGNavMeshBuilder.Build(vertices, new int[] { 0, 1, 2 }, highSlope);
            Equal(1, kept.TriangleCount, "45 degree triangle must be kept at a 60 degree slope limit");
        }

        #endregion

        #region Helpers

        private sealed class GridData
        {
            public FPVector3[] Vertices;
            public int[] Indices;
        }

        private static GridData BuildGrid(int width, int depth, int cellSize)
        {
            GridData grid = new GridData();
            grid.Vertices = new FPVector3[(width + 1) * (depth + 1)];
            for (int z = 0; z <= depth; z++)
            {
                for (int x = 0; x <= width; x++)
                {
                    int index = (z * (width + 1)) + x;
                    grid.Vertices[index] = V(x * cellSize, 0, z * cellSize);
                }
            }

            grid.Indices = new int[width * depth * 6];
            int cursor = 0;
            for (int z = 0; z < depth; z++)
            {
                for (int x = 0; x < width; x++)
                {
                    int v00 = (z * (width + 1)) + x;
                    int v01 = v00 + 1;
                    int v10 = v00 + (width + 1);
                    int v11 = v10 + 1;

                    grid.Indices[cursor++] = v00;
                    grid.Indices[cursor++] = v01;
                    grid.Indices[cursor++] = v10;

                    grid.Indices[cursor++] = v10;
                    grid.Indices[cursor++] = v01;
                    grid.Indices[cursor++] = v11;
                }
            }

            return grid;
        }

        private static CYGNavBuildConfig CreateGridConfig(int triangleCount)
        {
            CYGNavBuildConfig config = new CYGNavBuildConfig();
            config.SlopeLimitDegrees = FP.FromInt(60);
            config.TriangleRegions = new int[triangleCount];
            config.TriangleAreaMasks = new uint[triangleCount];
            config.TriangleCosts = new FP[triangleCount];
            for (int i = 0; i < triangleCount; i++)
            {
                config.TriangleRegions[i] = 0;
                config.TriangleAreaMasks[i] = 1u;
                config.TriangleCosts[i] = FP.One;
            }

            return config;
        }

        private static void SetCellRegion(CYGNavBuildConfig config, int width, int x, int z, int region)
        {
            int firstTriangle = ((z * width) + x) * 2;
            config.TriangleRegions[firstTriangle] = region;
            config.TriangleRegions[firstTriangle + 1] = region;
        }

        private static CYGAgentSystem CreateCrossingSystem(CYGNavMesh mesh)
        {
            CYGAgentSystem system = new CYGAgentSystem(mesh);
            CYGAgent a = system.AddAgent(
                V(2, 0, 2), FP.FromFraction(2, 5), FP.FromFraction(5, 2), FP.FromInt(8));
            CYGAgent b = system.AddAgent(
                V(6, 0, 2), FP.FromFraction(2, 5), FP.FromFraction(5, 2), FP.FromInt(8));
            CYGAgent c = system.AddAgent(
                V(1, 0, 6), FP.FromFraction(3, 10), FP.FromFraction(11, 5), FP.FromInt(7));

            a.SetDestination(V(6, 0, 6));
            b.SetDestination(V(2, 0, 6));
            c.SetDestination(V(6, 0, 1));
            return system;
        }

        private static FPVector3 V(int x, int y, int z)
        {
            return new FPVector3(FP.FromInt(x), FP.FromInt(y), FP.FromInt(z));
        }

        private static FPVector3 V(int x, FP y, int z)
        {
            return new FPVector3(FP.FromInt(x), y, FP.FromInt(z));
        }

        private static FP HorizontalDistance(FPVector3 a, FPVector3 b)
        {
            FP dx = a.x - b.x;
            FP dz = a.z - b.z;
            return FP.Sqrt((dx * dx) + (dz * dz));
        }

        private static FP PathLength(FPVector3 start, CYGPathResult path)
        {
            FPVector3 previous = start;
            FP total = FP.Zero;
            for (int i = 0; i < path.WaypointCount; i++)
            {
                FPVector3 waypoint = path.GetWaypoint(i);
                total = total + FPVector3.Distance(previous, waypoint);
                previous = waypoint;
            }

            return total;
        }

        private static bool ContainsWaypoint(CYGPathResult path, FPVector3 point)
        {
            FP tolerance = FP.FromFraction(1, 100);
            for (int i = 0; i < path.WaypointCount; i++)
            {
                if (FPVector3.Distance(path.GetWaypoint(i), point) <= tolerance)
                    return true;
            }

            return false;
        }

        private static bool HasConsecutiveTriangles(CYGPathResult path, int first, int second)
        {
            for (int i = 0; i < path.TrianglePathCount - 1; i++)
            {
                if (path.GetTriangle(i) == first && path.GetTriangle(i + 1) == second)
                    return true;
            }

            return false;
        }

        private static void AssertPathAvoidsRegion(CYGNavMesh mesh, CYGPathResult path, int region)
        {
            for (int i = 0; i < path.TrianglePathCount; i++)
            {
                CYGNavMeshTriangle triangle;
                mesh.TryGetTriangle(path.GetTriangle(i), out triangle);
                Require(triangle.RegionId != region, "path must avoid blocked region " + region);
            }
        }

        private static void AssertSamePath(CYGPathResult first, CYGPathResult second, string label)
        {
            _assertions++;
            if (first.Status != second.Status)
                throw new InvalidOperationException(label + ": status differs");

            Equal(first.WaypointCount, second.WaypointCount, label + ": waypoint count");
            for (int i = 0; i < first.WaypointCount; i++)
                Require(first.GetWaypoint(i) == second.GetWaypoint(i), label + ": waypoint " + i + " differs");

            Equal(first.TrianglePathCount, second.TrianglePathCount, label + ": triangle path count");
            for (int i = 0; i < first.TrianglePathCount; i++)
                Equal(first.GetTriangle(i), second.GetTriangle(i), label + ": triangle " + i + " differs");

            Equal(first.PathCost, second.PathCost, label + ": path cost");
            Equal(first.StartTriangle, second.StartTriangle, label + ": start triangle");
            Equal(first.EndTriangle, second.EndTriangle, label + ": end triangle");
        }

        private static void Require(bool condition, string message)
        {
            _assertions++;
            if (!condition)
                throw new InvalidOperationException(message);
        }

        private static void Equal(int expected, int actual, string message)
        {
            _assertions++;
            if (expected != actual)
                throw new InvalidOperationException(message + " (expected " + expected + ", actual " + actual + ")");
        }

        private static void Equal(uint expected, uint actual, string message)
        {
            _assertions++;
            if (expected != actual)
                throw new InvalidOperationException(message + " (expected " + expected + ", actual " + actual + ")");
        }

        private static void Equal(FP expected, FP actual, string message)
        {
            _assertions++;
            if (expected != actual)
                throw new InvalidOperationException(
                    message + " (expected raw " + expected.RawValue + ", actual raw " + actual.RawValue + ")");
        }

        private static void Throws<T>(Action action, string message) where T : Exception
        {
            _assertions++;
            try
            {
                action();
            }
            catch (T)
            {
                return;
            }
            catch (Exception exception)
            {
                throw new InvalidOperationException(
                    message + " (wrong exception " + exception.GetType().Name + ")");
            }

            throw new InvalidOperationException(message + " (no exception)");
        }

        #endregion
    }
}
