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

        private CYGPhysicsBody _ballBody;

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

        /// <summary>
        /// CRC32 of every piece of simulation state that can affect a future
        /// tick. The Unity view state (animations, camera, HUD message) is not
        /// included. Physics.Tick is intentionally excluded because it is an
        /// internal accumulator, not gameplay state; the ball body pose and
        /// motion are hashed instead.
        /// </summary>
        public uint ComputeStateHash()
        {
            uint crc = 0xFFFFFFFFu;
            CYGChecksum.HashInt(ref crc, Tick);
            CYGChecksum.HashInt(ref crc, (int)Phase);
            MatchTime.ComputeChecksum(ref crc);
            PhaseTimer.ComputeChecksum(ref crc);

            for (int i = 0; i < Scores.Length; i++)
            {
                CYGChecksum.HashInt(ref crc, Scores[i]);
            }

            for (int i = 0; i < Players.Length; i++)
            {
                SportsArenaPlayer player = Players[i];
                if (player == null)
                {
                    continue;
                }

                CYGChecksum.HashInt(ref crc, player.Index);
                CYGChecksum.HashInt(ref crc, player.Team);
                player.Position.ComputeChecksum(ref crc);
                player.Velocity.ComputeChecksum(ref crc);
                player.Yaw.ComputeChecksum(ref crc);
                HashBool(ref crc, player.Grounded);
                HashBool(ref crc, player.Blocking);
                HashBool(ref crc, player.HasBall);
                HashBool(ref crc, player.IsBot);
                player.Health.ComputeChecksum(ref crc);
                player.PunchCooldown.ComputeChecksum(ref crc);
                player.PunchTimer.ComputeChecksum(ref crc);
                player.DashCooldown.ComputeChecksum(ref crc);
                player.DashTimer.ComputeChecksum(ref crc);
                player.ThrowCooldown.ComputeChecksum(ref crc);
                player.CatchCooldown.ComputeChecksum(ref crc);
                player.StunTimer.ComputeChecksum(ref crc);
                player.RespawnTimer.ComputeChecksum(ref crc);
                player.AnimSpeed.ComputeChecksum(ref crc);
                CYGChecksum.HashByte(ref crc, player.PreviousButtons);

                FPCharacterController controller = _controllers[i];
                if (controller != null)
                {
                    controller.Position.ComputeChecksum(ref crc);
                    controller.Velocity.ComputeChecksum(ref crc);
                    HashBool(ref crc, controller.IsGrounded);
                    controller.GroundNormal.ComputeChecksum(ref crc);
                }
            }

            Ball.Position.ComputeChecksum(ref crc);
            Ball.Velocity.ComputeChecksum(ref crc);
            CYGChecksum.HashInt(ref crc, Ball.Holder);
            CYGChecksum.HashInt(ref crc, Ball.LastThrower);
            CYGChecksum.HashInt(ref crc, Ball.BodyId);
            HashBool(ref crc, Ball.Active);
            Ball.CatchLock.ComputeChecksum(ref crc);

            CYGPhysicsBody ballBody = _ballBody;
            if (ballBody != null)
            {
                HashBool(ref crc, ballBody.IsActive);
                HashBool(ref crc, ballBody.IsAwake);
                ballBody.Rotation.ComputeChecksum(ref crc);
                ballBody.AngularVelocity.ComputeChecksum(ref crc);
            }

            return crc ^ 0xFFFFFFFFu;
        }

        private static void HashBool(ref uint crc, bool value)
        {
            CYGChecksum.HashByte(ref crc, value ? (byte)1 : (byte)0);
        }

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
