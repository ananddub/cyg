using System;

namespace CookYourGame.Sim.TwoBox
{
    /// <summary>
    /// Player box state. Serialization, type id and schema hash are generated
    /// from Samples/TwoBox/twobox.cyg in TwoBoxComponents.Generated.cs.
    /// </summary>
    public partial struct BoxPlayer
    {
        public CYGPlayerRef Player;
        public int Score;

        public BoxPlayer(CYGPlayerRef player, int score = 0)
        {
            Player = player;
            Score = score;
        }
    }

    /// <summary>Movement state for a player box.</summary>
    public partial struct BoxMovement
    {
        public FP Speed;
        public FPVector3 Velocity;

        public BoxMovement(FP speed)
        {
            Speed = speed;
            Velocity = FPVector3.Zero;
        }
    }

    /// <summary>Golden gem target. Radius is the gem's half extent on X/Z.</summary>
    public partial struct GemTarget
    {
        public FP Radius;
        public FPVector3 Position;
        public bool IsActive;

        public GemTarget(FP radius, FPVector3 position, bool isActive = true)
        {
            Radius = radius;
            Position = position;
            IsActive = isActive;
        }
    }

    /// <summary>
    /// View event emitted when a gem is collected. Route is local because the
    /// view layer consumes it on the simulating peer; the event buffer still
    /// deduplicates it across rollbacks.
    /// </summary>
    public sealed class GemCollectedEvent : CYGEvent
    {
        public const int EventTypeId = 1000001;

        public override int TypeId => EventTypeId;
        public override CYGEventRoute Route => CYGEventRoute.Local;

        public CYGEntityRef Collector { get; }
        public CYGPlayerRef Player { get; }
        public FPVector3 Position { get; }
        public int NewScore { get; }

        public GemCollectedEvent(CYGEntityRef collector, CYGPlayerRef player, FPVector3 position, int newScore)
        {
            Collector = collector;
            Player = player;
            Position = position;
            NewScore = newScore;
        }
    }
}
