using System;

namespace CookYourGame.Sim.Headless
{
    public enum CYGMatchMode
    {
        Kinematic,
        Physics3D,
        Physics2D,
        Navigation,
    }

    /// <summary>
    /// Builds identical worlds for the server and both clients. Entity
    /// creation order, system order and spawn transforms are fixed here so
    /// only the inputs can make the three simulations diverge.
    /// </summary>
    internal static class CYGMatchWorldFactory
    {
        public static void Build(
            CYGMatchMode mode,
            CYGDeterminismConfig config,
            CYGNavMesh navMesh,
            out CYGWorld world)
        {
            world = new CYGWorld(
                rngSeed: config.Seed,
                maxPlayers: config.MaxPlayers,
                commandCapacity: 64,
                eventRetentionTicks: 2048);

            // Health/state component is present in every mode, matching the
            // Unity runner which always registers and spawns CYGSimAgent.
            world.Register<CYGSimAgent>();

            switch (mode)
            {
                case CYGMatchMode.Kinematic:
                    SpawnAgentOnly(world, 1, new FPVector3(FP.FromInt(-3), FP.Zero, FP.Zero));
                    SpawnAgentOnly(world, 2, new FPVector3(FP.FromInt(3), FP.Zero, FP.Zero));
                    break;

                case CYGMatchMode.Physics3D:
                    BuildPhysics3D(world);
                    break;

                case CYGMatchMode.Physics2D:
                    BuildPhysics2D(world);
                    break;

                case CYGMatchMode.Navigation:
                    if (navMesh == null)
                    {
                        throw new ArgumentNullException(nameof(navMesh));
                    }
                    BuildNavigation(world, navMesh);
                    break;

                default:
                    throw new ArgumentOutOfRangeException(nameof(mode));
            }
        }

        public static CYGSystemRunner CreateSystems(
            CYGWorld world,
            CYGDeterminismConfig config,
            CYGMatchMode mode,
            CYGNavMesh navMesh)
        {
            CYGSystemRunner systems = new CYGSystemRunner(world, config);
            switch (mode)
            {
                case CYGMatchMode.Kinematic:
                    systems.AddSystem(new CYGCharacterMovementSystem());
                    break;

                case CYGMatchMode.Physics3D:
                    CYGPhysics3DSystem physics3D = new CYGPhysics3DSystem();
                    physics3D.MoveSpeed = FP.FromInt(3);
                    physics3D.JumpImpulse = FP.FromInt(4);
                    systems.AddSystem(physics3D);
                    break;

                case CYGMatchMode.Physics2D:
                    CYGPhysics2DSystem physics2D = new CYGPhysics2DSystem();
                    physics2D.MoveSpeed = FP.FromInt(3);
                    physics2D.JumpImpulse = FP.FromInt(4);
                    systems.AddSystem(physics2D);
                    break;

                case CYGMatchMode.Navigation:
                    if (navMesh == null)
                    {
                        throw new ArgumentNullException(nameof(navMesh));
                    }
                    systems.AddSystem(new CYGNavigationSystem(navMesh));
                    break;

                default:
                    throw new ArgumentOutOfRangeException(nameof(mode));
            }
            return systems;
        }

        public static CYGNavMesh BuildGridNavMesh()
        {
            const int width = 6;
            const int depth = 6;
            const int cell = 2;

            FPVector3[] vertices = new FPVector3[(width + 1) * (depth + 1)];
            for (int z = 0; z <= depth; z++)
            {
                for (int x = 0; x <= width; x++)
                {
                    vertices[(z * (width + 1)) + x] = new FPVector3(
                        FP.FromInt(x * cell), FP.Zero, FP.FromInt(z * cell));
                }
            }

            int[] indices = new int[width * depth * 6];
            int cursor = 0;
            for (int z = 0; z < depth; z++)
            {
                for (int x = 0; x < width; x++)
                {
                    int v00 = (z * (width + 1)) + x;
                    int v01 = v00 + 1;
                    int v10 = v00 + (width + 1);
                    int v11 = v10 + 1;

                    indices[cursor++] = v00;
                    indices[cursor++] = v01;
                    indices[cursor++] = v10;
                    indices[cursor++] = v10;
                    indices[cursor++] = v01;
                    indices[cursor++] = v11;
                }
            }

            return CYGNavMeshBuilder.Build(vertices, indices);
        }

        private static void BuildPhysics3D(CYGWorld world)
        {
            world.Register<CYGPhysicsBody3D>();

            CYGEntityRef ground = world.CreateEntity();
            world.Set(ground, CYGPhysicsBody3D.StaticBox(
                new FPVector3(FP.Zero, -FP.FromFraction(1, 2), FP.Zero),
                new FPVector3(FP.FromInt(24), FP.FromFraction(1, 2), FP.FromInt(24))));

            FPVector3 firstPosition = new FPVector3(FP.FromInt(-2), FP.FromInt(2), FP.Zero);
            FPVector3 secondPosition = new FPVector3(FP.FromInt(2), FP.FromInt(2), FP.Zero);

            CYGEntityRef first = world.CreateEntity();
            world.Set(first, CYGPhysicsBody3D.DynamicSphere(
                1, firstPosition, FP.FromFraction(1, 2), FP.One));
            world.Set(first, CYGSimAgent.Create(1, firstPosition));

            CYGEntityRef second = world.CreateEntity();
            world.Set(second, CYGPhysicsBody3D.DynamicSphere(
                2, secondPosition, FP.FromFraction(1, 2), FP.One));
            world.Set(second, CYGSimAgent.Create(2, secondPosition));
        }

        private static void BuildPhysics2D(CYGWorld world)
        {
            world.Register<CYGPhysicsBody2D>();

            CYGEntityRef ground = world.CreateEntity();
            world.Set(ground, CYGPhysicsBody2D.StaticBox(
                new FPVector2(FP.Zero, -FP.FromFraction(1, 2)),
                new FPVector2(FP.FromInt(24), FP.FromFraction(1, 2))));

            FPVector2 firstPosition = new FPVector2(FP.FromInt(-2), FP.FromInt(2));
            FPVector2 secondPosition = new FPVector2(FP.FromInt(2), FP.FromInt(2));

            CYGEntityRef first = world.CreateEntity();
            world.Set(first, CYGPhysicsBody2D.DynamicCircle(
                1, firstPosition, FP.FromFraction(1, 2), FP.One));
            world.Set(first, CYGSimAgent.Create(
                1, new FPVector3(firstPosition.x, FP.Zero, firstPosition.y)));

            CYGEntityRef second = world.CreateEntity();
            world.Set(second, CYGPhysicsBody2D.DynamicCircle(
                2, secondPosition, FP.FromFraction(1, 2), FP.One));
            world.Set(second, CYGSimAgent.Create(
                2, new FPVector3(secondPosition.x, FP.Zero, secondPosition.y)));
        }

        private static void BuildNavigation(CYGWorld world, CYGNavMesh navMesh)
        {
            world.Register<CYGNavAgent>();

            FPVector3 firstPosition = new FPVector3(FP.One, FP.Zero, FP.One);
            FPVector3 secondPosition = new FPVector3(FP.FromInt(5), FP.Zero, FP.FromInt(5));

            CYGEntityRef first = world.CreateEntity();
            world.Set(first, CYGNavAgent.Create(1, firstPosition));
            world.Set(first, CYGSimAgent.Create(1, firstPosition));

            CYGEntityRef second = world.CreateEntity();
            world.Set(second, CYGNavAgent.Create(2, secondPosition));
            world.Set(second, CYGSimAgent.Create(2, secondPosition));
        }

        private static void SpawnAgentOnly(CYGWorld world, int playerId, FPVector3 position)
        {
            CYGEntityRef entity = world.CreateEntity();
            world.Set(entity, CYGSimAgent.Create(playerId, position));
        }
    }
}
