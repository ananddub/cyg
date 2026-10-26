namespace CookYourGame.Sim
{
    /// <summary>Routing intent for a view event. The view layer applies local/remote filtering.</summary>
    public enum CYGEventRoute
    {
        Synced,
        NoHash,
        Local,
        Remote,
        Client,
        Server
    }

    /// <summary>
    /// View-only event fired from a simulation system. Events never affect
    /// simulation state; they are buffered so rollback can discard predicted
    /// events, and keys make re-emitted events exactly-once.
    /// </summary>
    public abstract class CYGEvent
    {
        public abstract int TypeId { get; }

        public virtual CYGEventRoute Route
        {
            get { return CYGEventRoute.Synced; }
        }

        /// <summary>When true, dispatch waits until the firing tick is confirmed.</summary>
        public virtual bool VerifiedOnly
        {
            get { return Route == CYGEventRoute.Synced || Route == CYGEventRoute.NoHash; }
        }

        public int Tick { get; internal set; }

        public CYGEventKey Key { get; internal set; }
    }
}
