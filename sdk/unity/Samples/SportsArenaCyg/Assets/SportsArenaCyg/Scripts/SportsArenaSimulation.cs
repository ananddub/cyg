using System;
using CookYourGame.Sim;

namespace CookYourGame.SportsArena
{
    /// <summary>
    /// Deterministic Sports Arena Brawler rules running on CookYourGame.Sim.
    /// Movement uses CYGCharacterController-style capsule sweeps, the ball uses
    /// CYGPhysicsWorld, and all gameplay state is fixed-point.
    /// </summary>
    public sealed partial class SportsArenaSimulation
    {
        public readonly SportsArenaPlayer[] Players = new SportsArenaPlayer[SportsArenaConfig.PlayerCount];
        public readonly SportsArenaBall Ball = new SportsArenaBall();
        public readonly CYGPhysicsWorld Physics = new CYGPhysicsWorld();

        private readonly FPCharacterController[] _controllers = new FPCharacterController[SportsArenaConfig.PlayerCount];
        private readonly SportsArenaSnapshot _snapshot = new SportsArenaSnapshot();
        private readonly FPVector3[] _spawns = new FPVector3[SportsArenaConfig.PlayerCount];

        public SportsArenaPhase Phase { get; private set; } = SportsArenaPhase.Countdown;
        public int[] Scores { get; } = new int[SportsArenaConfig.PlayerCount];
        public FP MatchTime { get; private set; } = SportsArenaConfig.MatchDuration;
        public FP PhaseTimer { get; private set; } = FP.FromInt(3);
        public FPAABB ArenaBounds { get; private set; }
        public SportsArenaGoal[] Goals { get; private set; } = Array.Empty<SportsArenaGoal>();
        public FPVector3 BallSpawn { get; private set; }
        public string Message { get; private set; } = "GET READY";
        public int Tick { get; private set; }

        public SportsArenaSimulation()
        {
            Physics.Settings.Gravity = SportsArenaConfig.Gravity;
            Physics.Settings.FixedDeltaTime = SportsArenaConfig.DeltaTime;
            Physics.Settings.EnableSleeping = false;
            Physics.Settings.EnableCcd = true;
        }

        public void TickStep(SportsArenaInput[] inputs)
        {
            Tick++;
            switch (Phase)
            {
                case SportsArenaPhase.Countdown:
                    PhaseTimer -= SportsArenaConfig.DeltaTime;
                    StepBall();
                    if (PhaseTimer <= FP.Zero)
                    {
                        Phase = SportsArenaPhase.Playing;
                        Message = string.Empty;
                    }
                    break;
                case SportsArenaPhase.Playing:
                    MatchTime -= SportsArenaConfig.DeltaTime;
                    if (MatchTime <= FP.Zero)
                    {
                        MatchTime = FP.Zero;
                        EndMatch();
                        break;
                    }
                    StepPlayers(inputs);
                    StepBall();
                    CheckGoals();
                    break;
                case SportsArenaPhase.GoalScored:
                    PhaseTimer -= SportsArenaConfig.DeltaTime;
                    StepPlayers(inputs);
                    if (PhaseTimer <= FP.Zero)
                    {
                        ResetRound();
                        Phase = SportsArenaPhase.Playing;
                    }
                    break;
                case SportsArenaPhase.GameOver:
                    PhaseTimer -= SportsArenaConfig.DeltaTime;
                    if (PhaseTimer <= FP.Zero)
                    {
                        RestartMatch();
                    }
                    break;
            }
            WriteSnapshot();
        }

        public SportsArenaSnapshot Snapshot => _snapshot;

        public void AddStaticBox(FPVector3 center, FPVector3 halfExtents, FPQuaternion rotation)
        {
            CYGPhysicsBody body = CYGPhysicsBody.CreateStatic(CYGCollider.Box(halfExtents));
            body.Position = center;
            body.Rotation = rotation;
            body.Friction = FP.FromFraction(4, 5);
            Physics.AddBody(body);
        }

        public void AddStaticMesh(CYGStaticMesh mesh)
        {
            if (mesh == null)
            {
                return;
            }
            Physics.AddStaticMesh(mesh);
        }

        private void WriteSnapshot()
        {
            _snapshot.Phase = Phase;
            _snapshot.MatchTime = MatchTime;
            _snapshot.Message = Message;
            _snapshot.BallPosition = Ball.Position;
            _snapshot.BallActive = Ball.Active;
            _snapshot.BallHolder = Ball.Holder;
            for (int i = 0; i < Players.Length; i++)
            {
                SportsArenaPlayer p = Players[i];
                _snapshot.Scores[i] = Scores[i];
                _snapshot.Health[i] = p.Health;
                _snapshot.Positions[i] = p.Position;
                _snapshot.Yaw[i] = p.Yaw;
                _snapshot.Grounded[i] = p.Grounded;
                _snapshot.HasBall[i] = p.HasBall;
                _snapshot.Stunned[i] = p.IsStunned;
                _snapshot.Blocking[i] = p.Blocking;
                _snapshot.AnimSpeed[i] = p.AnimSpeed;
                _snapshot.Triggers[i] = p.AnimTriggers;
                p.AnimTriggers = 0;
            }
        }
    }
}
