//-----------------------------------------------------------------------------
// CYGNavMeshBuilder.cs
// Deterministic bake from triangle soup into CYGNavMesh.
// Zero UnityEngine dependencies, all math is Q32.32 fixed point.
//
// LIMITATIONS (MVP):
//  - Agent radius/height are recorded but not used for erosion or clearance.
//    Callers must author walkable triangles with the required margin.
//  - Triangles are dropped when degenerate or steeper than the slope limit.
//    There is no voxelization, hole filling, or tile stitching.
//  - Off-mesh link points are not validated against triangle geometry.
//-----------------------------------------------------------------------------

using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Bake inputs. Per-triangle arrays are indexed by input triangle
    /// (indices.Length / 3). Missing entries use region 0, area bit 0, and
    /// DefaultCost. Off-mesh links reference input triangle indices; links to
    /// dropped triangles are removed from the baked mesh.
    /// </summary>
    public sealed class CYGNavBuildConfig
    {
        public FP SlopeLimitDegrees;
        public FP AgentRadius;
        public FP AgentHeight;
        public FP DefaultCost;
        public int[] TriangleRegions;
        public uint[] TriangleAreaMasks;
        public FP[] TriangleCosts;
        public CYGOffMeshLink[] Links;

        public CYGNavBuildConfig()
        {
            SlopeLimitDegrees = FP.FromInt(45);
            AgentRadius = FP.Zero;
            AgentHeight = FP.Zero;
            DefaultCost = FP.One;
        }
    }

    /// <summary>
    /// Deterministic navmesh baker. Output ordering depends only on input
    /// ordering: triangles are kept in ascending input order and each
    /// triangle's neighbour list is sorted ascending with open edges last.
    /// </summary>
    public static class CYGNavMeshBuilder
    {
        private static readonly FP SlopeTolerance = FP.FromFraction(1, 10000);
        private const uint DefaultAreaMask = 1u;

        public static CYGNavMesh Build(FPVector3[] vertices, int[] indices)
        {
            return Build(vertices, indices, null);
        }

        public static CYGNavMesh Build(FPVector3[] vertices, int[] indices, CYGNavBuildConfig config)
        {
            if (vertices == null)
                throw new ArgumentNullException(nameof(vertices));
            if (indices == null)
                throw new ArgumentNullException(nameof(indices));
            if ((indices.Length % 3) != 0)
                throw new ArgumentException("Triangle index count must be a multiple of three.", nameof(indices));

            if (config == null)
                config = new CYGNavBuildConfig();

            int rawTriangleCount = indices.Length / 3;
            ValidateConfig(config, rawTriangleCount);

            FP slopeLimit = FP.Clamp(config.SlopeLimitDegrees, FP.Zero, FP.FromInt(90));
            FP slopeCosLimit = FP.Cos(slopeLimit * FP.Deg2Rad);

            List<CYGNavMeshTriangle> triangles = new List<CYGNavMeshTriangle>(rawTriangleCount);
            int[] rawToBaked = new int[rawTriangleCount];
            for (int i = 0; i < rawToBaked.Length; i++)
                rawToBaked[i] = -1;

            for (int inputTriangle = 0; inputTriangle < rawTriangleCount; inputTriangle++)
            {
                int offset = inputTriangle * 3;
                int i0 = indices[offset];
                int i1 = indices[offset + 1];
                int i2 = indices[offset + 2];

                ValidateIndex(i0, vertices.Length, inputTriangle);
                ValidateIndex(i1, vertices.Length, inputTriangle);
                ValidateIndex(i2, vertices.Length, inputTriangle);

                if (i0 == i1 || i1 == i2 || i2 == i0)
                    continue;

                FPVector3 a = vertices[i0];
                FPVector3 b = vertices[i1];
                FPVector3 c = vertices[i2];
                FPVector3 normal = FPVector3.Cross(b - a, c - a);
                FP lengthSquared = normal.SqrMagnitude;
                if (lengthSquared <= FP.Zero)
                    continue;

                // Enforce upward winding so shared-edge portals and the funnel
                // have a single, deterministic orientation.
                if (normal.y < FP.Zero)
                {
                    int swap = i1;
                    i1 = i2;
                    i2 = swap;
                    b = vertices[i1];
                    c = vertices[i2];
                    normal = FPVector3.Cross(b - a, c - a);
                    lengthSquared = normal.SqrMagnitude;
                    if (lengthSquared <= FP.Zero)
                        continue;
                }

                FP normalLength = FP.Sqrt(lengthSquared);
                FP slopeCos = normal.y / normalLength;
                if (slopeCos < slopeCosLimit - SlopeTolerance)
                    continue;

                int regionId = config.TriangleRegions != null ? config.TriangleRegions[inputTriangle] : 0;
                uint areaMask = config.TriangleAreaMasks != null ? config.TriangleAreaMasks[inputTriangle] : DefaultAreaMask;
                FP cost = config.TriangleCosts != null ? config.TriangleCosts[inputTriangle] : config.DefaultCost;

                CYGNavMeshTriangle triangle = new CYGNavMeshTriangle(i0, i1, i2, regionId, areaMask, cost);
                rawToBaked[inputTriangle] = triangles.Count;
                triangles.Add(triangle);
            }

            int triangleCount = triangles.Count;
            CYGNavMeshTriangle[] bakedTriangles = triangles.ToArray();
            int[] bakedIndices = new int[triangleCount * 3];
            FPVector3[] centroids = new FPVector3[triangleCount];
            FPAABB bounds = new FPAABB(FPVector3.Zero, FPVector3.Zero);
            bool hasBounds = false;

            for (int i = 0; i < triangleCount; i++)
            {
                CYGNavMeshTriangle triangle = bakedTriangles[i];
                bakedIndices[i * 3] = triangle.Vertex0;
                bakedIndices[(i * 3) + 1] = triangle.Vertex1;
                bakedIndices[(i * 3) + 2] = triangle.Vertex2;

                FPVector3 a = vertices[triangle.Vertex0];
                FPVector3 b = vertices[triangle.Vertex1];
                FPVector3 c = vertices[triangle.Vertex2];
                centroids[i] = new FPVector3(
                    (a.x + b.x + c.x) / 3,
                    (a.y + b.y + c.y) / 3,
                    (a.z + b.z + c.z) / 3);

                if (!hasBounds)
                {
                    bounds = new FPAABB(a, a);
                    hasBounds = true;
                }

                bounds.Encapsulate(b);
                bounds.Encapsulate(c);
            }

            int[] neighbors = BuildNeighbors(bakedTriangles);
            CYGOffMeshLink[] links = BuildLinks(config.Links, rawToBaked);

            // Copy author data so later caller mutation cannot change the bake.
            FPVector3[] vertexCopy = (FPVector3[])vertices.Clone();

            return new CYGNavMesh(vertexCopy, bakedIndices, bakedTriangles, centroids, neighbors, links, bounds);
        }

        private static void ValidateConfig(CYGNavBuildConfig config, int rawTriangleCount)
        {
            if (config.TriangleRegions != null)
            {
                if (config.TriangleRegions.Length != rawTriangleCount)
                    throw new ArgumentException("TriangleRegions length must match the triangle count.", nameof(config));

                for (int i = 0; i < config.TriangleRegions.Length; i++)
                {
                    int region = config.TriangleRegions[i];
                    if (region < 0 || region >= 32)
                        throw new ArgumentException("Triangle region ids must be in [0, 31].", nameof(config));
                }
            }

            if (config.TriangleAreaMasks != null && config.TriangleAreaMasks.Length != rawTriangleCount)
                throw new ArgumentException("TriangleAreaMasks length must match the triangle count.", nameof(config));

            if (config.TriangleCosts != null)
            {
                if (config.TriangleCosts.Length != rawTriangleCount)
                    throw new ArgumentException("TriangleCosts length must match the triangle count.", nameof(config));

                for (int i = 0; i < config.TriangleCosts.Length; i++)
                {
                    if (config.TriangleCosts[i] < FP.Zero)
                        throw new ArgumentException("Triangle costs must be non-negative.", nameof(config));
                }
            }

            if (config.DefaultCost < FP.Zero)
                throw new ArgumentException("DefaultCost must be non-negative.", nameof(config));

            if (config.AgentRadius < FP.Zero || config.AgentHeight < FP.Zero)
                throw new ArgumentException("Agent radius/height must be non-negative.", nameof(config));
        }

        private static void ValidateIndex(int index, int vertexCount, int triangle)
        {
            if (index < 0 || index >= vertexCount)
            {
                throw new ArgumentException(
                    "Triangle " + triangle + " references vertex " + index
                    + " but the vertex array has " + vertexCount + " entries.");
            }
        }

        private static int[] BuildNeighbors(CYGNavMeshTriangle[] triangles)
        {
            int triangleCount = triangles.Length;
            int[] neighbors = new int[triangleCount * 3];
            for (int i = 0; i < neighbors.Length; i++)
                neighbors[i] = -1;

            // Keyed by the sorted vertex pair. Only lookups are used, never
            // enumeration, so Dictionary hash seed cannot affect output.
            Dictionary<long, int> firstEdgeOwner = new Dictionary<long, int>();

            for (int triangle = 0; triangle < triangleCount; triangle++)
            {
                CYGNavMeshTriangle current = triangles[triangle];
                for (int edge = 0; edge < 3; edge++)
                {
                    int a = current.GetVertexIndex(edge);
                    int b = current.GetVertexIndex((edge + 1) % 3);
                    long key = EdgeKey(a, b);
                    int encodedOwner;

                    if (!firstEdgeOwner.TryGetValue(key, out encodedOwner))
                    {
                        firstEdgeOwner.Add(key, (triangle * 3) + edge);
                        continue;
                    }

                    int ownerTriangle = encodedOwner / 3;
                    int ownerEdge = encodedOwner % 3;
                    int currentSlot = (triangle * 3) + edge;
                    int ownerSlot = (ownerTriangle * 3) + ownerEdge;

                    // A third triangle on the same non-manifold edge is ignored.
                    if (neighbors[currentSlot] < 0 && neighbors[ownerSlot] < 0)
                    {
                        neighbors[currentSlot] = ownerTriangle;
                        neighbors[ownerSlot] = triangle;
                    }
                }
            }

            // Stable sorted neighbour list: ascending triangle index, then -1.
            for (int triangle = 0; triangle < triangleCount; triangle++)
            {
                int baseIndex = triangle * 3;
                for (int i = 0; i < 2; i++)
                {
                    for (int j = i + 1; j < 3; j++)
                    {
                        if (SortKey(neighbors[baseIndex + j]) < SortKey(neighbors[baseIndex + i]))
                        {
                            int swap = neighbors[baseIndex + i];
                            neighbors[baseIndex + i] = neighbors[baseIndex + j];
                            neighbors[baseIndex + j] = swap;
                        }
                    }
                }
            }

            return neighbors;
        }

        private static int SortKey(int neighbor)
        {
            return neighbor < 0 ? int.MaxValue : neighbor;
        }

        private static long EdgeKey(int a, int b)
        {
            int low = a < b ? a : b;
            int high = a < b ? b : a;
            return ((long)low << 32) | (uint)high;
        }

        private static CYGOffMeshLink[] BuildLinks(CYGOffMeshLink[] input, int[] rawToBaked)
        {
            if (input == null || input.Length == 0 || rawToBaked.Length == 0)
                return new CYGOffMeshLink[0];

            List<CYGOffMeshLink> links = new List<CYGOffMeshLink>(input.Length);
            for (int i = 0; i < input.Length; i++)
            {
                CYGOffMeshLink link = input[i];
                if (link.FromTriangle < 0 || link.FromTriangle >= rawToBaked.Length)
                    throw new ArgumentException("Off-mesh link " + i + " has an invalid FromTriangle.");
                if (link.ToTriangle < 0 || link.ToTriangle >= rawToBaked.Length)
                    throw new ArgumentException("Off-mesh link " + i + " has an invalid ToTriangle.");
                if (link.Cost < FP.Zero)
                    throw new ArgumentException("Off-mesh link " + i + " has a negative cost.");

                int from = rawToBaked[link.FromTriangle];
                int to = rawToBaked[link.ToTriangle];
                if (from < 0 || to < 0 || from == to)
                    continue;

                link.FromTriangle = from;
                link.ToTriangle = to;
                links.Add(link);
            }

            return links.ToArray();
        }
    }
}
