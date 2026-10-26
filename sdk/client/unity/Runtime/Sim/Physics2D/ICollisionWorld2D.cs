//-----------------------------------------------------------------------------
// ICollisionWorld2D.cs
// Read-only spatial query surface shared by 2D physics implementations.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

namespace CookYourGame.Sim
{
    /// <summary>
    /// Deterministic spatial queries over a 2D collision world.
    /// Results are ordered by distance (nearest first) and then by ascending
    /// body id, so equal distances never depend on storage order.
    /// </summary>
    public interface ICollisionWorld2D
    {
        /// <summary>Nearest ray hit against all layers. Includes triggers.</summary>
        bool Raycast(
            FPVector2 origin,
            FPVector2 direction,
            FP maxDistance,
            out CYGPhysics2DRaycastHit hit);

        /// <summary>Nearest ray hit against the layer mask. Includes triggers.</summary>
        bool Raycast(
            FPVector2 origin,
            FPVector2 direction,
            FP maxDistance,
            int layerMask,
            out CYGPhysics2DRaycastHit hit);

        /// <summary>
        /// Bodies overlapping a circle, written nearest first then by ascending
        /// body id. Returns the total match count even if the buffer is small.
        /// </summary>
        int OverlapCircle(FPVector2 center, FP radius, int layerMask, int[] results);

        /// <summary>
        /// Bodies whose shape overlaps the axis-aligned box [min, max], written
        /// nearest first then by ascending body id. Returns the total match count.
        /// </summary>
        int OverlapAABB(FPVector2 min, FPVector2 max, int layerMask, int[] results);
    }
}
