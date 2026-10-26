using CookYourGame.Sim;

namespace CookYourGame.SportsArena
{
    public enum SportsArenaPhase
    {
        Countdown,
        Playing,
        GoalScored,
        GameOver,
    }

    public sealed class SportsArenaPlayer
    {
        public int Index;
        public int Team;
        public FPVector3 Position;
        public FPVector3 Velocity;
        public FP Yaw;
        public bool Grounded;
        public bool Blocking;
        public bool HasBall;
        public bool IsBot;
        public FP Health;
        public FP PunchCooldown;
        public FP PunchTimer;
        public FP DashCooldown;
        public FP DashTimer;
        public FP ThrowCooldown;
        public FP CatchCooldown;
        public FP StunTimer;
        public FP RespawnTimer;
        public FP AnimSpeed;
        public byte AnimTriggers;
        public byte PreviousButtons;

        public bool IsStunned => StunTimer > FP.Zero;
        public bool IsRespawning => RespawnTimer > FP.Zero;
    }

    public sealed class SportsArenaBall
    {
        public FPVector3 Position;
        public FPVector3 Velocity;
        public int Holder = -1;
        public int LastThrower = -1;
        public int BodyId = -1;
        public bool Active;
        public FP CatchLock;
    }

    /// <summary>
    /// Small deterministic result object handed to the Unity view layer. It is
    /// re-used every tick so the view path does not allocate.
    /// </summary>
    public sealed class SportsArenaSnapshot
    {
        public SportsArenaPhase Phase;
        public FP MatchTime;
        public int[] Scores = new int[SportsArenaConfig.PlayerCount];
        public FP[] Health = new FP[SportsArenaConfig.PlayerCount];
        public FPVector3[] Positions = new FPVector3[SportsArenaConfig.PlayerCount];
        public FP[] Yaw = new FP[SportsArenaConfig.PlayerCount];
        public bool[] Grounded = new bool[SportsArenaConfig.PlayerCount];
        public bool[] HasBall = new bool[SportsArenaConfig.PlayerCount];
        public bool[] Stunned = new bool[SportsArenaConfig.PlayerCount];
        public bool[] Blocking = new bool[SportsArenaConfig.PlayerCount];
        public FP[] AnimSpeed = new FP[SportsArenaConfig.PlayerCount];
        public byte[] Triggers = new byte[SportsArenaConfig.PlayerCount];
        public FPVector3 BallPosition;
        public bool BallActive;
        public int BallHolder;
        public string Message = string.Empty;
    }

    internal static class SportsArenaAnim
    {
        public const byte Jump = 1 << 0;
        public const byte Dash = 1 << 1;
        public const byte Punch = 1 << 2;
        public const byte ThrowShort = 1 << 3;
        public const byte ThrowLong = 1 << 4;
        public const byte Catch = 1 << 5;
        public const byte Stun = 1 << 6;
    }
}
