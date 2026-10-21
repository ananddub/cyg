using System;
using CookYourGame.Sim;

namespace CookYourGame.SportsArena
{
    public sealed partial class SportsArenaSimulation
    {
        public void Start(FPAABB arenaBounds, SportsArenaGoal[] goals, FPVector3[] spawns, FPVector3 ballSpawn)
        {
            ArenaBounds = arenaBounds;
            Goals = goals ?? Array.Empty<SportsArenaGoal>();
            BallSpawn = ballSpawn;

            for (int i = 0; i < Players.Length; i++)
            {
                SportsArenaPlayer player = new SportsArenaPlayer();
                player.Index = i;
                player.Team = i % 2;
                player.Position = i < spawns.Length ? spawns[i] : ballSpawn;
                player.Yaw = player.Team == 0 ? FP.FromInt(90) : FP.FromInt(-90);
                player.Health = SportsArenaConfig.MaxHealth;
                player.IsBot = i >= 2;
                Players[i] = player;
                _spawns[i] = player.Position;
                _controllers[i] = new FPCharacterController(
                    player.Position,
                    SportsArenaConfig.PlayerRadius,
                    SportsArenaConfig.PlayerHeight);
            }

            CreateBallBody();
            ResetBall();
            Phase = SportsArenaPhase.Countdown;
            PhaseTimer = FP.FromInt(3);
            Message = "GET READY";
        }

        private void CreateBallBody()
        {
            CYGPhysicsBody body = CYGPhysicsBody.CreateDynamic(
                CYGCollider.Sphere(SportsArenaConfig.BallRadius),
                SportsArenaConfig.BallMass);
            body.Friction = SportsArenaConfig.BallFriction;
            body.Restitution = SportsArenaConfig.BallRestitution;
            body.GravityScale = SportsArenaConfig.BallGravityScale;
            body.EnableCcd = true;
            body.IsActive = false;
            Ball.BodyId = Physics.AddBody(body);
        }

        private void ResetBall()
        {
            Ball.Holder = -1;
            Ball.LastThrower = -1;
            Ball.Active = true;
            Ball.CatchLock = FP.Zero;
            Ball.Position = BallSpawn;
            Ball.Velocity = FPVector3.Zero;
            CYGPhysicsBody body = Physics.GetBody(Ball.BodyId);
            if (body != null)
            {
                body.IsActive = true;
                body.Position = BallSpawn;
                body.LinearVelocity = FPVector3.Zero;
                body.AngularVelocity = FPVector3.Zero;
                body.Rotation = FPQuaternion.Identity;
                body.Wake();
            }
        }

        private void ResetRound()
        {
            ResetBall();
            for (int i = 0; i < Players.Length; i++)
            {
                SportsArenaPlayer player = Players[i];
                player.Position = _spawns[i];
                player.Velocity = FPVector3.Zero;
                player.Health = SportsArenaConfig.MaxHealth;
                player.Grounded = false;
                player.HasBall = false;
                player.Blocking = false;
                player.StunTimer = FP.Zero;
                player.RespawnTimer = FP.Zero;
                player.PunchCooldown = FP.Zero;
                player.DashCooldown = FP.Zero;
                player.ThrowCooldown = FP.Zero;
                player.CatchCooldown = FP.Zero;
                _controllers[i].Position = player.Position;
                _controllers[i].Velocity = FPVector3.Zero;
            }
            Message = string.Empty;
        }

        private void RestartMatch()
        {
            for (int i = 0; i < Scores.Length; i++)
            {
                Scores[i] = 0;
            }
            MatchTime = SportsArenaConfig.MatchDuration;
            ResetRound();
            Phase = SportsArenaPhase.Countdown;
            PhaseTimer = FP.FromInt(3);
            Message = "GET READY";
        }

        private void EndMatch()
        {
            Phase = SportsArenaPhase.GameOver;
            PhaseTimer = FP.FromInt(3);
            if (Scores[0] > Scores[1])
            {
                Message = "BLUE WINS";
            }
            else if (Scores[1] > Scores[0])
            {
                Message = "RED WINS";
            }
            else
            {
                Message = "DRAW";
            }
        }

        private void CheckGoals()
        {
            if (!Ball.Active || Ball.Holder >= 0)
            {
                return;
            }

            for (int i = 0; i < Goals.Length; i++)
            {
                if (!Goals[i].Area.Contains(Ball.Position))
                {
                    continue;
                }

                int scorer = 1 - Goals[i].DefendingTeam;
                Scores[scorer]++;
                Message = scorer == 0 ? "BLUE SCORES" : "RED SCORES";
                Phase = SportsArenaPhase.GoalScored;
                PhaseTimer = FP.FromInt(3);
                Ball.Active = false;
                CYGPhysicsBody body = Physics.GetBody(Ball.BodyId);
                if (body != null)
                {
                    body.IsActive = false;
                }
                if (Scores[scorer] >= SportsArenaConfig.ScoreToWin)
                {
                    EndMatch();
                }
                return;
            }
        }
    }
}
