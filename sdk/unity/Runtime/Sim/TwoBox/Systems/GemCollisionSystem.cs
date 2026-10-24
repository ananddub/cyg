using System;

namespace CookYourGame.Sim.TwoBox
{
    /// <summary>
    /// Deterministic gem pickup. Uses an AABB overlap against the 1.2-unit
    /// player box instead of a center-distance check, collects at most once
    /// per gem per tick, and respawns away from every active player using the
    /// world RNG (restored by rollback like any other state).
    /// </summary>
    public sealed class GemCollisionSystem : CYGSystem
    {
        private static readonly FP PlayerHalfExtent = FP.FromFraction(3, 5);
        private static readonly FP DefaultGemHalfExtent = FP.FromFraction(7, 20);
        private static readonly FP RespawnClearance = FP.FromInt(2);

        public override void OnUpdate(in CYGSystemContext context)
        {
            CYGWorld world = context.World;
            int slots = world.SlotCount;

            for (int i = 0; i < slots; i++)
            {
                if (!world.TryGetEntityAt(i, out CYGEntityRef gemEntity))
                {
                    continue;
                }
                if (!world.TryGet(gemEntity, out GemTarget gem) || !gem.IsActive)
                {
                    continue;
                }

                for (int j = 0; j < slots; j++)
                {
                    if (!world.TryGetEntityAt(j, out CYGEntityRef playerEntity))
                    {
                        continue;
                    }
                    if (!world.TryGet(playerEntity, out BoxPlayer player) ||
                        !world.TryGet(playerEntity, out CYGSimAgent agent))
                    {
                        continue;
                    }

                    if (!Overlaps(agent.Position, gem.Position, gem.Radius))
                    {
                        continue;
                    }

                    player.Score += 1;
                    world.Set(playerEntity, player);

                    gem.Position = PickRespawn(world, gem.Position);
                    gem.IsActive = true;
                    world.Set(gemEntity, gem);

                    world.Events.Publish(new GemCollectedEvent(
                        playerEntity,
                        player.Player,
                        gem.Position,
                        player.Score));

                    break;
                }
            }
        }

        private static bool Overlaps(FPVector3 playerPosition, FPVector3 gemPosition, FP gemRadius)
        {
            FP half = gemRadius > FP.Zero ? gemRadius : DefaultGemHalfExtent;
            FP reach = PlayerHalfExtent + half;

            FP dx = playerPosition.x - gemPosition.x;
            if (dx < FP.Zero) dx = -dx;
            FP dy = playerPosition.y - gemPosition.y;
            if (dy < FP.Zero) dy = -dy;
            FP dz = playerPosition.z - gemPosition.z;
            if (dz < FP.Zero) dz = -dz;

            return dx <= reach && dy <= reach && dz <= reach;
        }

        private static FPVector3 PickRespawn(CYGWorld world, FPVector3 current)
        {
            FPVector3 candidate = current;
            for (int attempt = 0; attempt < 8; attempt++)
            {
                int rx = world.Rng.NextInt(-5500, 5501);
                int rz = world.Rng.NextInt(-5500, 5501);
                candidate = new FPVector3(
                    FP.FromFraction(rx, 1000),
                    current.y,
                    FP.FromFraction(rz, 1000));

                if (!IsNearPlayer(world, candidate))
                {
                    return candidate;
                }
            }

            return candidate;
        }

        private static bool IsNearPlayer(CYGWorld world, FPVector3 position)
        {
            FP clearanceSq = RespawnClearance * RespawnClearance;
            int slots = world.SlotCount;
            for (int i = 0; i < slots; i++)
            {
                if (!world.TryGetEntityAt(i, out CYGEntityRef entity))
                {
                    continue;
                }
                if (!world.Has<BoxPlayer>(entity) || !world.TryGet(entity, out CYGSimAgent agent))
                {
                    continue;
                }

                FP dx = agent.Position.x - position.x;
                FP dz = agent.Position.z - position.z;
                if (dx * dx + dz * dz < clearanceSq)
                {
                    return true;
                }
            }
            return false;
        }
    }
}
