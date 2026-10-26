using System;

namespace CookYourGame.Sim.TwoBox
{
    /// <summary>
    /// Deterministic ECS system for player movement.
    /// Runs deterministically across all connected clients.
    /// Filters entities with BoxPlayer + BoxMovement + CYGSimAgent.
    /// </summary>
    public sealed class TwoBoxMovementSystem : CYGSystemMainThreadFilter<TwoBoxMovementSystem.Filter>
    {
        public override void OnInit(CYGWorld world)
        {
            if (!world.TryGetStore(BoxPlayer.TypeId, out _)) world.Register<BoxPlayer>();
            if (!world.TryGetStore(BoxMovement.TypeId, out _)) world.Register<BoxMovement>();
            if (!world.TryGetStore(GemTarget.TypeId, out _)) world.Register<GemTarget>();
            if (!world.TryGetStore(CYGSimAgent.TypeId, out _)) world.Register<CYGSimAgent>();

            // Deterministic initial spawn of arena actors at tick 0
            if (world.EntityCount == 0)
            {
                // Player 1 (Blue)
                CYGEntityRef p1 = world.CreateEntity();
                world.Set(p1, CYGSimAgent.Create(1, new FPVector3(FP.FromFloat(-3.5f), FP.FromFraction(1, 2), FP.Zero)));
                world.Set(p1, new BoxPlayer { Player = new CYGPlayerRef(1), Score = 0 });
                world.Set(p1, new BoxMovement { Speed = FP.FromInt(6) });

                // Player 2 (Orange)
                CYGEntityRef p2 = world.CreateEntity();
                world.Set(p2, CYGSimAgent.Create(2, new FPVector3(FP.FromFloat(3.5f), FP.FromFraction(1, 2), FP.Zero)));
                world.Set(p2, new BoxPlayer { Player = new CYGPlayerRef(2), Score = 0 });
                world.Set(p2, new BoxMovement { Speed = FP.FromInt(6) });

                // Golden Gem Target
                CYGEntityRef gem = world.CreateEntity();
                world.Set(gem, new GemTarget
                {
                    Radius = FP.FromFloat(1.2f),
                    Position = new FPVector3(FP.Zero, FP.FromFloat(0.6f), FP.Zero),
                    IsActive = true
                });
            }
        }

        public struct Filter : ICYGSystemFilter
        {
            public bool Matches(CYGWorld world, CYGEntityRef entity)
            {
                return world.Has<BoxPlayer>(entity) &&
                       world.Has<BoxMovement>(entity) &&
                       world.Has<CYGSimAgent>(entity);
            }
        }

        protected override void UpdateEntity(in CYGSystemContext context, CYGEntityRef entity)
        {
            if (!context.World.TryGet(entity, out BoxPlayer player) ||
                !context.World.TryGet(entity, out BoxMovement movement) ||
                !context.World.TryGet(entity, out CYGSimAgent agent))
            {
                return;
            }

            // Look up deterministic input submitted for this player
            if (context.TryGetInput((int)player.Player.Value, out CYGPlayerInput input))
            {
                // Convert input axes (-1000..1000) into normalized fixed-point direction
                FP moveX = FP.FromFraction(input.MoveX, 1000);
                FP moveZ = FP.FromFraction(input.MoveY, 1000);

                FPVector3 moveDir = new FPVector3(moveX, FP.Zero, moveZ);
                if (moveDir.SqrMagnitude > FP.One)
                {
                    moveDir = moveDir.Normalized;
                }

                FP speed = movement.Speed > FP.Zero ? movement.Speed : FP.FromFloat(6.0f);
                agent.Velocity = moveDir * speed;
                agent.Position += agent.Velocity * context.DeltaTime;

                // Deterministic boundary clamping inside arena bounds (-6.2 .. +6.2)
                FP bound = FP.FromFloat(6.2f);
                if (agent.Position.x < -bound) agent.Position.x = -bound;
                if (agent.Position.x > bound) agent.Position.x = bound;
                if (agent.Position.z < -bound) agent.Position.z = -bound;
                if (agent.Position.z > bound) agent.Position.z = bound;

                // Keep Y at 0.5 (flush on floor plane)
                agent.Position.y = FP.FromFraction(1, 2);

                // Write back updated component state to deterministic ECS stores
                movement.Velocity = agent.Velocity;
                context.World.Set(entity, movement);
                context.World.Set(entity, agent);
            }
        }
    }
}
