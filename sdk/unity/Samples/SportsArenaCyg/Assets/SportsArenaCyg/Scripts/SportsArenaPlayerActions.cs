using CookYourGame.Sim;

namespace CookYourGame.SportsArena
{
    public sealed partial class SportsArenaSimulation
    {
        private void Punch(SportsArenaPlayer player)
        {
            player.PunchCooldown = SportsArenaConfig.PunchCooldown;
            player.PunchTimer = SportsArenaConfig.PunchDuration;
            player.AnimTriggers |= SportsArenaAnim.Punch;
            FPVector3 forward = SportsArenaConfig.Forward(player.Yaw);

            for (int i = 0; i < Players.Length; i++)
            {
                SportsArenaPlayer other = Players[i];
                if (other == player || other.IsRespawning || other.Team == player.Team)
                {
                    continue;
                }

                FPVector3 delta = other.Position - player.Position;
                delta.y = FP.Zero;
                if (delta.SqrMagnitude > SportsArenaConfig.PunchRange * SportsArenaConfig.PunchRange)
                {
                    continue;
                }
                if (delta.SqrMagnitude > FP.Zero
                    && FPVector3.Dot(delta.Normalized, forward) < FP.FromFraction(2, 5))
                {
                    continue;
                }

                FP scale = other.Blocking ? FP.FromFraction(1, 4) : FP.One;
                other.Health -= SportsArenaConfig.PunchDamage * scale;
                other.StunTimer = SportsArenaConfig.StunDuration * scale;
                other.Velocity += forward * (SportsArenaConfig.PunchKnockback * scale)
                    + FPVector3.Up * (FP.FromInt(4) * scale);
                other.AnimTriggers |= SportsArenaAnim.Stun;
                if (other.HasBall)
                {
                    DropBall(other, forward * FP.FromInt(3));
                }
                if (other.Health <= FP.Zero)
                {
                    BeginRespawn(other);
                }
            }

            if (Ball.Active && Ball.Holder < 0)
            {
                FPVector3 delta = Ball.Position - player.Position;
                if (delta.SqrMagnitude <= SportsArenaConfig.PunchRange * SportsArenaConfig.PunchRange)
                {
                    Ball.Velocity += forward * FP.FromInt(10) + FPVector3.Up * FP.FromInt(4);
                }
            }
        }

        private void ThrowBall(SportsArenaPlayer player)
        {
            FPVector3 forward = SportsArenaConfig.Forward(player.Yaw);
            FPVector3 position = player.Position + FPVector3.Up * FP.FromFraction(6, 5) + forward * FP.FromFraction(3, 5);
            FPVector3 velocity = forward * SportsArenaConfig.ThrowSpeed
                + FPVector3.Up * SportsArenaConfig.ThrowUp
                + new FPVector3(player.Velocity.x * FP.FromFraction(1, 3), FP.Zero,
                    player.Velocity.z * FP.FromFraction(1, 3));
            Ball.Holder = -1;
            Ball.Active = true;
            Ball.Position = position;
            Ball.Velocity = velocity;
            Ball.LastThrower = player.Index;
            Ball.CatchLock = SportsArenaConfig.CatchCooldown;
            player.HasBall = false;
            player.ThrowCooldown = SportsArenaConfig.ThrowCooldown;
            player.AnimTriggers |= SportsArenaAnim.ThrowShort;

            CYGPhysicsBody body = Physics.GetBody(Ball.BodyId);
            if (body != null)
            {
                body.IsActive = true;
                body.Position = position;
                body.LinearVelocity = velocity;
                body.AngularVelocity = FPVector3.Zero;
                body.Rotation = FPQuaternion.Identity;
                body.Wake();
            }
        }

        private void DropBall(SportsArenaPlayer player, FPVector3 impulse)
        {
            if (!player.HasBall)
            {
                return;
            }
            player.HasBall = false;
            Ball.Holder = -1;
            Ball.Active = true;
            Ball.Position = player.Position + FPVector3.Up * FP.One;
            Ball.Velocity = impulse + FPVector3.Up * FP.FromInt(2);
            Ball.CatchLock = SportsArenaConfig.CatchCooldown;
            CYGPhysicsBody body = Physics.GetBody(Ball.BodyId);
            if (body != null)
            {
                body.IsActive = true;
                body.Position = Ball.Position;
                body.LinearVelocity = Ball.Velocity;
                body.Wake();
            }
        }

        private void BeginRespawn(SportsArenaPlayer player)
        {
            player.RespawnTimer = SportsArenaConfig.RespawnDelay;
            player.Health = SportsArenaConfig.MaxHealth;
            player.Velocity = FPVector3.Zero;
            player.StunTimer = FP.Zero;
            if (Ball.Holder == player.Index)
            {
                DropBall(player, FPVector3.Zero);
            }
            player.HasBall = false;
        }

        private void Respawn(SportsArenaPlayer player)
        {
            player.Position = _spawns[player.Index];
            player.Velocity = FPVector3.Zero;
            player.Health = SportsArenaConfig.MaxHealth;
            player.RespawnTimer = FP.Zero;
            player.StunTimer = FP.Zero;
            player.Grounded = false;
            _controllers[player.Index].Position = player.Position;
            _controllers[player.Index].Velocity = FPVector3.Zero;
        }

        private static bool IsIdle(SportsArenaInput input)
        {
            return input.Value.MoveX == 0 && input.Value.MoveY == 0 && input.Value.Buttons == 0;
        }

        private SportsArenaInput BuildBotInput(SportsArenaPlayer bot)
        {
            SportsArenaPlayer opponent = NearestOpponent(bot);
            FPVector3 target;
            if (bot.HasBall)
            {
                target = GoalCenterFor(bot);
            }
            else if (Ball.Holder >= 0)
            {
                target = Players[Ball.Holder].Position;
            }
            else if (Ball.Active)
            {
                target = Ball.Position;
            }
            else
            {
                target = opponent != null ? opponent.Position : FPVector3.Zero;
            }
            FPVector3 delta = target - bot.Position;
            delta.y = FP.Zero;
            FP distance = delta.Magnitude;
            FPVector3 direction = distance > FP.Zero ? delta / distance : FPVector3.Zero;
            short x = (short)FP.Clamp(direction.x * 1000, -1000, 1000).AsInt();
            short z = (short)FP.Clamp(direction.z * 1000, -1000, 1000).AsInt();
            byte buttons = 0;
            if (!bot.HasBall && distance < SportsArenaConfig.PunchRange && bot.PunchCooldown <= FP.Zero)
            {
                buttons |= CYGPlayerInput.ButtonAction1;
            }
            if (bot.HasBall && distance < FP.FromInt(10) && bot.ThrowCooldown <= FP.Zero)
            {
                buttons |= CYGPlayerInput.ButtonAction3;
            }
            if (distance > FP.FromInt(4) && bot.DashCooldown <= FP.Zero && Ball.Active)
            {
                buttons |= CYGPlayerInput.ButtonAction2;
            }
            return new SportsArenaInput
            {
                Value = new CYGPlayerInput(Tick, buttons, x, z),
            };
        }

        private SportsArenaPlayer NearestOpponent(SportsArenaPlayer player)
        {
            SportsArenaPlayer nearest = null;
            FP best = FP.MaxValue;
            for (int i = 0; i < Players.Length; i++)
            {
                SportsArenaPlayer other = Players[i];
                if (other.Team == player.Team)
                {
                    continue;
                }
                FP distance = FPVector3.DistanceSquared(player.Position, other.Position);
                if (distance < best)
                {
                    best = distance;
                    nearest = other;
                }
            }
            return nearest;
        }

        private FPVector3 GoalCenterFor(SportsArenaPlayer player)
        {
            if (Goals.Length == 0)
            {
                return FPVector3.Zero;
            }
            for (int i = 0; i < Goals.Length; i++)
            {
                if (Goals[i].DefendingTeam != player.Team)
                {
                    return Goals[i].Area.Center;
                }
            }
            return FPVector3.Zero;
        }

        private static FPVector3 MoveTowards(FPVector3 current, FPVector3 target, FP maxDelta)
        {
            FPVector3 delta = target - current;
            FP sqr = delta.SqrMagnitude;
            if (sqr <= maxDelta * maxDelta || sqr <= FP.Zero)
            {
                return target;
            }
            return current + delta.Normalized * maxDelta;
        }

        private static FP TurnTowards(FP current, FP target, FP maxDelta)
        {
            FP delta = target - current;
            while (delta > FP.FromInt(180)) delta -= FP.FromInt(360);
            while (delta < FP.FromInt(-180)) delta += FP.FromInt(360);
            if (delta > maxDelta) delta = maxDelta;
            if (delta < -maxDelta) delta = -maxDelta;
            return current + delta;
        }
    }
}
