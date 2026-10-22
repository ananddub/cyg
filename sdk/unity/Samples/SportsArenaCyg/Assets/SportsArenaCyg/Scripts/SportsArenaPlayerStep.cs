using CookYourGame.Sim;

namespace CookYourGame.SportsArena
{
    public sealed partial class SportsArenaSimulation
    {
        private void StepPlayers(SportsArenaInput[] inputs)
        {
            for (int i = 0; i < Players.Length; i++)
            {
                SportsArenaPlayer player = Players[i];
                SportsArenaInput input = inputs != null && i < inputs.Length
                    ? inputs[i]
                    : default;

                if (player.IsBot && IsIdle(input))
                {
                    input = BuildBotInput(player);
                }

                StepPlayer(player, input);
            }

            ResolvePlayerBallPickup();
        }

        private void StepPlayer(SportsArenaPlayer player, SportsArenaInput input)
        {
            FP dt = SportsArenaConfig.DeltaTime;
            player.PunchCooldown = FP.Max(FP.Zero, player.PunchCooldown - dt);
            player.DashCooldown = FP.Max(FP.Zero, player.DashCooldown - dt);
            player.ThrowCooldown = FP.Max(FP.Zero, player.ThrowCooldown - dt);
            player.CatchCooldown = FP.Max(FP.Zero, player.CatchCooldown - dt);
            player.PunchTimer = FP.Max(FP.Zero, player.PunchTimer - dt);
            player.DashTimer = FP.Max(FP.Zero, player.DashTimer - dt);
            player.StunTimer = FP.Max(FP.Zero, player.StunTimer - dt);

            if (player.RespawnTimer > FP.Zero)
            {
                player.RespawnTimer -= dt;
                if (player.RespawnTimer <= FP.Zero)
                {
                    Respawn(player);
                }
                return;
            }

            byte buttons = input.Value.Buttons;
            bool jumpPressed = (buttons & CYGPlayerInput.ButtonJump) != 0
                && (player.PreviousButtons & CYGPlayerInput.ButtonJump) == 0;
            bool punchPressed = (buttons & CYGPlayerInput.ButtonAction1) != 0
                && (player.PreviousButtons & CYGPlayerInput.ButtonAction1) == 0;
            bool dashPressed = (buttons & CYGPlayerInput.ButtonAction2) != 0
                && (player.PreviousButtons & CYGPlayerInput.ButtonAction2) == 0;
            bool throwPressed = (buttons & CYGPlayerInput.ButtonAction3) != 0
                && (player.PreviousButtons & CYGPlayerInput.ButtonAction3) == 0;
            player.PreviousButtons = buttons;

            player.Blocking = input.Block && player.Grounded && !player.IsStunned;
            player.AnimSpeed = FP.Zero;
            FPCharacterController controller = _controllers[player.Index];
            controller.Position = player.Position;
            bool grounded = controller.CheckGrounded(Physics);

            if (!player.IsStunned && punchPressed && player.PunchCooldown <= FP.Zero)
            {
                Punch(player);
            }

            if (!player.IsStunned && dashPressed && player.DashCooldown <= FP.Zero)
            {
                FPVector3 dashDirection = input.Move.SqrMagnitude > FP.FromFraction(1, 100)
                    ? input.Move.Normalized
                    : SportsArenaConfig.Forward(player.Yaw);
                player.Velocity = new FPVector3(
                    dashDirection.x * SportsArenaConfig.DashSpeed,
                    FP.Zero,
                    dashDirection.z * SportsArenaConfig.DashSpeed);
                player.DashTimer = SportsArenaConfig.DashDuration;
                player.DashCooldown = SportsArenaConfig.DashCooldown;
                player.AnimTriggers |= SportsArenaAnim.Dash;
            }

            if (!player.IsStunned && throwPressed && player.HasBall && player.ThrowCooldown <= FP.Zero)
            {
                ThrowBall(player);
            }

            if (!player.IsStunned && jumpPressed && grounded)
            {
                player.Velocity.y = SportsArenaConfig.JumpImpulse;
                player.AnimTriggers |= SportsArenaAnim.Jump;
            }

            if (player.DashTimer <= FP.Zero)
            {
                FPVector3 desired = player.IsStunned || player.IsRespawning
                    ? FPVector3.Zero
                    : input.Move;
                if (desired.SqrMagnitude > FP.One)
                {
                    desired = desired.Normalized;
                }
                FP speed = player.Blocking
                    ? SportsArenaConfig.MoveSpeed * FP.FromFraction(2, 5)
                    : SportsArenaConfig.MoveSpeed;
                desired *= speed;

                FPVector3 horizontal = new FPVector3(player.Velocity.x, FP.Zero, player.Velocity.z);
                FP acceleration = grounded
                    ? SportsArenaConfig.GroundAcceleration
                    : SportsArenaConfig.AirAcceleration;
                horizontal = MoveTowards(horizontal, desired, acceleration * dt);
                player.Velocity.x = horizontal.x;
                player.Velocity.z = horizontal.z;
                player.AnimSpeed = horizontal.Magnitude / SportsArenaConfig.MoveSpeed;
            }
            else
            {
                player.AnimSpeed = FP.One;
            }

            if (grounded && player.Velocity.y <= FP.Zero)
            {
                player.Velocity.y = FP.Zero;
            }
            player.Velocity.y += SportsArenaConfig.Gravity.y * dt;

            FPVector3 displacement = player.Velocity * dt;
            controller.Velocity = player.Velocity;
            controller.Move(displacement, Physics);
            player.Position = controller.Position;
            player.Velocity = controller.Velocity;
            player.Grounded = controller.IsGrounded || controller.CheckGrounded(Physics);

            if (player.Grounded && player.Velocity.y < FP.Zero)
            {
                player.Velocity.y = FP.Zero;
            }

            FPVector3 move = input.Move;
            if (move.SqrMagnitude > FP.FromFraction(1, 100))
            {
                FP target = FP.Atan2(move.x, move.z) * FP.Rad2Deg;
                player.Yaw = TurnTowards(player.Yaw, target, FP.FromInt(720) * dt);
            }

            if (player.Position.y < ArenaBounds.Min.y - FP.FromInt(6))
            {
                Respawn(player);
            }

        }

        private void ResolvePlayerBallPickup()
        {
            if (!Ball.Active || Ball.Holder >= 0 || Ball.CatchLock > FP.Zero)
            {
                return;
            }

            for (int i = 0; i < Players.Length; i++)
            {
                SportsArenaPlayer player = Players[i];
                if (player.IsRespawning || player.IsStunned || player.CatchCooldown > FP.Zero)
                {
                    continue;
                }
                if (FPVector3.Distance(player.Position + FPVector3.Up, Ball.Position)
                    > SportsArenaConfig.CatchRadius)
                {
                    continue;
                }

                Ball.Holder = i;
                Ball.Active = false;
                Ball.LastThrower = i;
                Ball.CatchLock = SportsArenaConfig.CatchCooldown;
                player.HasBall = true;
                player.CatchCooldown = SportsArenaConfig.CatchCooldown;
                player.AnimTriggers |= SportsArenaAnim.Catch;
                CYGPhysicsBody body = _ballBody;
                if (body != null)
                {
                    body.IsActive = false;
                }
                return;
            }
        }

    }
}
