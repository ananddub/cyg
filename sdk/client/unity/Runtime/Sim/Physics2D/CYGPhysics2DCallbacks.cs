//-----------------------------------------------------------------------------
// CYGPhysics2DCallbacks.cs
// Deterministic collision / trigger event sink for the 2D physics world.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

namespace CookYourGame.Sim
{
    /// <summary>
    /// Receives deterministic pair events from <see cref="CYGPhysics2DWorld"/>.
    /// Body ids are always passed in ascending order (bodyA &lt; bodyB).
    /// Callbacks are invoked after the simulation step, in sorted pair order.
    /// Enter fires once on the first overlapping step, Stay on every subsequent
    /// overlapping step, and Exit once when the pair separates.
    /// </summary>
    public interface ICYGPhysics2DCallbacks
    {
        void OnCollisionEnter(int bodyA, int bodyB);
        void OnCollisionStay(int bodyA, int bodyB);
        void OnCollisionExit(int bodyA, int bodyB);
        void OnTriggerEnter(int bodyA, int bodyB);
        void OnTriggerStay(int bodyA, int bodyB);
        void OnTriggerExit(int bodyA, int bodyB);
    }
}
