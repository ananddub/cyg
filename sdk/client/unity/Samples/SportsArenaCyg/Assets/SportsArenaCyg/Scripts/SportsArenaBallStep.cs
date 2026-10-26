using CookYourGame.Sim;

namespace CookYourGame.SportsArena
{
    public sealed partial class SportsArenaSimulation
    {
        private void StepBall()
        {
            FP dt = SportsArenaConfig.DeltaTime;
            Ball.CatchLock = FP.Max(FP.Zero, Ball.CatchLock - dt);

            if (Ball.Holder >= 0)
            {
                SportsArenaPlayer holder = Players[Ball.Holder];
                if (holder.IsStunned || holder.IsRespawning || !holder.HasBall)
                {
                    DropBall(holder, FPVector3.Zero);
                }
                else
                {
                    FPVector3 forward = SportsArenaConfig.Forward(holder.Yaw);
                    Ball.Position = holder.Position
                        + FPVector3.Up * FP.FromFraction(6, 5)
                        + forward * FP.FromFraction(2, 5);
                    Ball.Velocity = holder.Velocity;
                    return;
                }
            }

            if (!Ball.Active)
            {
                return;
            }

            Physics.Step(dt);
            CYGPhysicsBody body = _ballBody;
            if (body == null)
            {
                return;
            }

            Ball.Position = body.Position;
            Ball.Velocity = body.LinearVelocity;
            if (Ball.Position.y < ArenaBounds.Min.y - FP.FromInt(8))
            {
                ResetBall();
                return;
            }

            FPVector3 margin = new FPVector3(FP.FromInt(4), FP.FromInt(2), FP.FromInt(4));
            if (Ball.Position.x < ArenaBounds.Min.x - margin.x
                || Ball.Position.x > ArenaBounds.Max.x + margin.x
                || Ball.Position.z < ArenaBounds.Min.z - margin.z
                || Ball.Position.z > ArenaBounds.Max.z + margin.z)
            {
                ResetBall();
            }
        }
    }
}
