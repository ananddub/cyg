using System;

namespace CookYourGame.Sim.Tests
{
    internal static class PhysicsNavigationBridgeTests
    {
        private static int _assertions;

        public static int AssertionCount
        {
            get { return _assertions; }
        }

        public static void RunAll()
        {
            TestCodecs();
            TestPhysics3DDeterministic();
            TestPhysics3DLateInputConverges();
            TestPhysics2DDeterministic();
            TestPhysics2DLateInputConverges();
            TestNavigationDeterministic();
            TestNavigationLateInputConverges();
        }

        private static void TestCodecs()
        {
            CYGPhysicsBody3D body3D = CYGPhysicsBody3D.DynamicBox(
                7,
                new FPVector3(FP.FromInt(1), FP.FromInt(2), FP.FromInt(3)),
                new FPVector3(FP.Half, FP.One, FP.Half),
                FP.FromInt(2));
            body3D.BodyId = 42;
            body3D.LinearVelocity = new FPVector3(FP.One, FP.Zero, FP.Zero);
            body3D.Rotation = FPQuaternion.Euler(FP.Zero, FP.FromInt(45), FP.Zero);

            byte[] buffer = new byte[body3D.WireSize];
            Equal(212, body3D.WriteTo(buffer, 0), "3D body wire size");
            CYGPhysicsBody3D copy3D = default(CYGPhysicsBody3D);
            copy3D.ReadFrom(buffer, 0);
            Equal(body3D.BodyId, copy3D.BodyId, "3D body id roundtrip");
            Equal(body3D.PlayerId, copy3D.PlayerId, "3D player id roundtrip");
            Equal(body3D.Position, copy3D.Position, "3D position roundtrip");
            Equal(body3D.Rotation, copy3D.Rotation, "3D rotation roundtrip");
            Equal(body3D.LinearVelocity, copy3D.LinearVelocity, "3D velocity roundtrip");
            Equal(body3D.Mass, copy3D.Mass, "3D mass roundtrip");
            Equal(Checksum(body3D), Checksum(copy3D), "3D checksum roundtrip");

            CYGPhysicsBody2D body2D = CYGPhysicsBody2D.DynamicCircle(3, new FPVector2(FP.One, FP.FromInt(2)), FP.Half, FP.One);
            body2D.BodyId = 9;
            body2D.Velocity = new FPVector2(FP.FromInt(-1), FP.Zero);
            buffer = new byte[body2D.WireSize];
            Equal(144, body2D.WriteTo(buffer, 0), "2D body wire size");
            CYGPhysicsBody2D copy2D = default(CYGPhysicsBody2D);
            copy2D.ReadFrom(buffer, 0);
            Equal(body2D.BodyId, copy2D.BodyId, "2D body id roundtrip");
            Equal(body2D.Position, copy2D.Position, "2D position roundtrip");
            Equal(body2D.Velocity, copy2D.Velocity, "2D velocity roundtrip");
            Equal(body2D.Radius, copy2D.Radius, "2D radius roundtrip");
            Equal(Checksum(body2D), Checksum(copy2D), "2D checksum roundtrip");

            CYGNavAgent agent = CYGNavAgent.Create(5, new FPVector3(FP.One, FP.Zero, FP.One));
            agent.AgentId = 11;
            agent.HasDestination = true;
            agent.Destination = new FPVector3(FP.FromInt(4), FP.Zero, FP.FromInt(4));
            buffer = new byte[agent.WireSize];
            Equal(132, agent.WriteTo(buffer, 0), "nav agent wire size");
            CYGNavAgent copyAgent = default(CYGNavAgent);
            copyAgent.ReadFrom(buffer, 0);
            Equal(agent.AgentId, copyAgent.AgentId, "agent id roundtrip");
            Equal(agent.PlayerId, copyAgent.PlayerId, "agent player id roundtrip");
            Equal(agent.Position, copyAgent.Position, "agent position roundtrip");
            Equal(agent.Destination, copyAgent.Destination, "agent destination roundtrip");
            Require(copyAgent.HasDestination, "agent destination flag roundtrip");
            Equal(Checksum(agent), Checksum(copyAgent), "agent checksum roundtrip");
        }

        private static void TestPhysics3DDeterministic()
        {
            CYGDeterminismConfig config = CreateConfig();
            CYGWorldRollbackSession first = CreatePhysics3DSession(config, false);
            CYGWorldRollbackSession second = CreatePhysics3DSession(config, false);

            for (int tick = 0; tick < 120; tick++)
            {
                CYGPlayerInput input = CreateLocalInput(tick);
                first.SetLocalInput(tick, input);
                second.SetLocalInput(tick, input);
                Require(first.Step(), "first 3D physics step");
                Require(second.Step(), "second 3D physics step");
            }

            Equal(first.Checksum, second.Checksum, "3D physics sessions must checksum equal");
            CYGPhysicsBody3D firstBody = FindPlayerBody(first.World, 1);
            CYGPhysicsBody3D secondBody = FindPlayerBody(second.World, 1);
            Equal(firstBody.Position, secondBody.Position, "3D body position must match across sessions");
            Equal(firstBody.LinearVelocity, secondBody.LinearVelocity, "3D body velocity must match across sessions");
            Require(firstBody.BodyId >= 0, "3D body id assigned");
        }

        private static void TestPhysics3DLateInputConverges()
        {
            CYGDeterminismConfig config = CreateConfig();
            CYGWorldRollbackSession reference = CreatePhysics3DSession(config, true);
            CYGWorldRollbackSession late = CreatePhysics3DSession(config, true);

            for (int tick = 0; tick < 8; tick++)
            {
                CYGPlayerInput local = CreateLocalInput(tick);
                reference.SetLocalInput(tick, local);
                late.SetLocalInput(tick, local);
                reference.OnRemoteInputReceived(2, CreateRemoteInput(tick));
            }

            for (int tick = 0; tick < 8; tick++)
            {
                Require(reference.Step(), "3D reference step");
                Require(late.Step(), "3D late step");
            }

            Require(reference.Checksum != late.Checksum, "missing 3D remote input must diverge");

            for (int tick = 0; tick < 8; tick++)
            {
                Require(late.OnRemoteInputReceived(2, CreateRemoteInput(tick)), "3D late input must roll back");
            }

            Equal(reference.Checksum, late.Checksum, "3D late input must converge");
            Equal(8, late.TotalRollbacks, "3D rollback count");
            Equal(36, late.TotalResimulatedTicks, "3D resimulation count");
            AssertFrameChecksumsEqual(reference, late, 8, "3D");
            Require(!late.VerifiedFrames.HasDesync, "3D corrected session has no desync");
        }

        private static void TestPhysics2DDeterministic()
        {
            CYGDeterminismConfig config = CreateConfig();
            CYGWorldRollbackSession first = CreatePhysics2DSession(config, false);
            CYGWorldRollbackSession second = CreatePhysics2DSession(config, false);

            for (int tick = 0; tick < 120; tick++)
            {
                CYGPlayerInput input = CreateLocalInput(tick);
                first.SetLocalInput(tick, input);
                second.SetLocalInput(tick, input);
                Require(first.Step(), "first 2D physics step");
                Require(second.Step(), "second 2D physics step");
            }

            Equal(first.Checksum, second.Checksum, "2D physics sessions must checksum equal");
            CYGPhysicsBody2D firstBody = FindPlayerBody2D(first.World, 1);
            CYGPhysicsBody2D secondBody = FindPlayerBody2D(second.World, 1);
            Equal(firstBody.Position, secondBody.Position, "2D body position must match");
            Equal(firstBody.Angle, secondBody.Angle, "2D body angle must match");
        }

        private static void TestPhysics2DLateInputConverges()
        {
            CYGDeterminismConfig config = CreateConfig();
            CYGWorldRollbackSession reference = CreatePhysics2DSession(config, true);
            CYGWorldRollbackSession late = CreatePhysics2DSession(config, true);

            for (int tick = 0; tick < 8; tick++)
            {
                CYGPlayerInput local = CreateLocalInput(tick);
                reference.SetLocalInput(tick, local);
                late.SetLocalInput(tick, local);
                reference.OnRemoteInputReceived(2, CreateRemoteInput(tick));
            }

            for (int tick = 0; tick < 8; tick++)
            {
                Require(reference.Step(), "2D reference step");
                Require(late.Step(), "2D late step");
            }

            Require(reference.Checksum != late.Checksum, "missing 2D remote input must diverge");

            for (int tick = 0; tick < 8; tick++)
            {
                Require(late.OnRemoteInputReceived(2, CreateRemoteInput(tick)), "2D late input must roll back");
            }

            Equal(reference.Checksum, late.Checksum, "2D late input must converge");
            AssertFrameChecksumsEqual(reference, late, 8, "2D");
            Require(!late.VerifiedFrames.HasDesync, "2D corrected session has no desync");
        }

        private static void TestNavigationDeterministic()
        {
            CYGDeterminismConfig config = CreateConfig();
            CYGNavMesh mesh = BuildGridMesh();
            CYGWorldRollbackSession first = CreateNavigationSession(config, mesh, false);
            CYGWorldRollbackSession second = CreateNavigationSession(config, mesh, false);

            for (int tick = 0; tick < 120; tick++)
            {
                CYGPlayerInput input = CreateLocalInput(tick);
                first.SetLocalInput(tick, input);
                second.SetLocalInput(tick, input);
                Require(first.Step(), "first navigation step");
                Require(second.Step(), "second navigation step");
            }

            Equal(first.Checksum, second.Checksum, "navigation sessions must checksum equal");
            CYGNavAgent firstAgent = FindAgent(first.World, 1);
            CYGNavAgent secondAgent = FindAgent(second.World, 1);
            Equal(firstAgent.Position, secondAgent.Position, "agent position must match across sessions");
            Equal(firstAgent.Velocity, secondAgent.Velocity, "agent velocity must match across sessions");
        }

        private static void TestNavigationLateInputConverges()
        {
            CYGDeterminismConfig config = CreateConfig();
            CYGNavMesh mesh = BuildGridMesh();
            CYGWorldRollbackSession reference = CreateNavigationSession(config, mesh, true);
            CYGWorldRollbackSession late = CreateNavigationSession(config, mesh, true);

            for (int tick = 0; tick < 8; tick++)
            {
                CYGPlayerInput local = CreateLocalInput(tick);
                reference.SetLocalInput(tick, local);
                late.SetLocalInput(tick, local);
                reference.OnRemoteInputReceived(2, CreateRemoteInput(tick));
            }

            for (int tick = 0; tick < 8; tick++)
            {
                Require(reference.Step(), "navigation reference step");
                Require(late.Step(), "navigation late step");
            }

            Require(reference.Checksum != late.Checksum, "missing navigation input must diverge");

            for (int tick = 0; tick < 8; tick++)
            {
                Require(late.OnRemoteInputReceived(2, CreateRemoteInput(tick)), "navigation late input must roll back");
            }

            Equal(reference.Checksum, late.Checksum, "navigation late input must converge");
            AssertFrameChecksumsEqual(reference, late, 8, "navigation");
            Require(!late.VerifiedFrames.HasDesync, "navigation corrected session has no desync");
        }

        private static void AssertFrameChecksumsEqual(
            CYGWorldRollbackSession reference,
            CYGWorldRollbackSession late,
            int ticks,
            string label)
        {
            for (int frame = 1; frame <= ticks; frame++)
            {
                uint expected;
                uint actual;
                Require(reference.VerifiedFrames.TryGetLocalChecksum(frame, out expected), label + " reference frame checksum");
                Require(late.VerifiedFrames.TryGetLocalChecksum(frame, out actual), label + " late frame checksum");
                Equal(expected, actual, label + " corrected frame checksum " + frame);
            }
        }

        private static CYGDeterminismConfig CreateConfig()
        {
            return new CYGDeterminismConfig(
                maxPlayers: 2,
                rollbackCapacity: 32,
                maxPredictionTicks: 8);
        }

        private static CYGWorldRollbackSession CreatePhysics3DSession(CYGDeterminismConfig config, bool twoPlayers)
        {
            CYGWorld world = new CYGWorld();
            CYGWorldRollbackSession session = new CYGWorldRollbackSession(
                world, config, defaultCharacterMovement: false);
            CYGSystemRunner systems = new CYGSystemRunner(world, config);
            systems.AddSystem(new CYGPhysics3DSystem());
            session.SetSystems(systems);

            AddEntity(world, CYGPhysicsBody3D.StaticBox(
                new FPVector3(FP.Zero, -FP.FromFraction(1, 2), FP.Zero),
                new FPVector3(FP.FromInt(20), FP.FromFraction(1, 2), FP.FromInt(20))));
            AddEntity(world, CYGPhysicsBody3D.DynamicSphere(
                1, new FPVector3(FP.Zero, FP.FromInt(2), FP.Zero), FP.FromFraction(1, 2), FP.One));
            if (twoPlayers)
            {
                AddEntity(world, CYGPhysicsBody3D.DynamicSphere(
                    2, new FPVector3(FP.FromInt(2), FP.FromInt(2), FP.Zero), FP.FromFraction(1, 2), FP.One));
            }

            return session;
        }

        private static CYGWorldRollbackSession CreatePhysics2DSession(CYGDeterminismConfig config, bool twoPlayers)
        {
            CYGWorld world = new CYGWorld();
            CYGWorldRollbackSession session = new CYGWorldRollbackSession(
                world, config, defaultCharacterMovement: false);
            CYGSystemRunner systems = new CYGSystemRunner(world, config);
            systems.AddSystem(new CYGPhysics2DSystem());
            session.SetSystems(systems);

            AddEntity(world, CYGPhysicsBody2D.StaticBox(
                new FPVector2(FP.Zero, -FP.FromFraction(1, 2)),
                new FPVector2(FP.FromInt(20), FP.FromFraction(1, 2))));
            AddEntity(world, CYGPhysicsBody2D.DynamicCircle(
                1, new FPVector2(FP.Zero, FP.FromInt(2)), FP.FromFraction(1, 2), FP.One));
            if (twoPlayers)
            {
                AddEntity(world, CYGPhysicsBody2D.DynamicCircle(
                    2, new FPVector2(FP.FromInt(2), FP.FromInt(2)), FP.FromFraction(1, 2), FP.One));
            }

            return session;
        }

        private static CYGWorldRollbackSession CreateNavigationSession(
            CYGDeterminismConfig config,
            CYGNavMesh mesh,
            bool twoPlayers)
        {
            CYGWorld world = new CYGWorld();
            CYGWorldRollbackSession session = new CYGWorldRollbackSession(
                world, config, defaultCharacterMovement: false);
            CYGSystemRunner systems = new CYGSystemRunner(world, config);
            systems.AddSystem(new CYGNavigationSystem(mesh));
            session.SetSystems(systems);

            AddEntity(world, CYGNavAgent.Create(1, new FPVector3(FP.One, FP.Zero, FP.One)));
            if (twoPlayers)
            {
                AddEntity(world, CYGNavAgent.Create(2, new FPVector3(FP.FromInt(5), FP.Zero, FP.FromInt(5))));
            }

            return session;
        }

        private static void AddEntity<T>(CYGWorld world, T component)
            where T : struct, ICYGComponent
        {
            CYGEntityRef entity = world.CreateEntity();
            world.Set(entity, component);
        }

        private static CYGPhysicsBody3D FindPlayerBody(CYGWorld world, int playerId)
        {
            for (int i = 0; i < world.SlotCount; i++)
            {
                CYGEntityRef entity;
                if (!world.TryGetEntityAt(i, out entity))
                {
                    continue;
                }
                CYGPhysicsBody3D body;
                if (world.TryGet(entity, out body) && body.PlayerId == playerId)
                {
                    return body;
                }
            }

            throw new InvalidOperationException("3D body for player " + playerId + " not found.");
        }

        private static CYGPhysicsBody2D FindPlayerBody2D(CYGWorld world, int playerId)
        {
            for (int i = 0; i < world.SlotCount; i++)
            {
                CYGEntityRef entity;
                if (!world.TryGetEntityAt(i, out entity))
                {
                    continue;
                }
                CYGPhysicsBody2D body;
                if (world.TryGet(entity, out body) && body.PlayerId == playerId)
                {
                    return body;
                }
            }

            throw new InvalidOperationException("2D body for player " + playerId + " not found.");
        }

        private static CYGNavAgent FindAgent(CYGWorld world, int playerId)
        {
            for (int i = 0; i < world.SlotCount; i++)
            {
                CYGEntityRef entity;
                if (!world.TryGetEntityAt(i, out entity))
                {
                    continue;
                }
                CYGNavAgent agent;
                if (world.TryGet(entity, out agent) && agent.PlayerId == playerId)
                {
                    return agent;
                }
            }

            throw new InvalidOperationException("Nav agent for player " + playerId + " not found.");
        }

        private static CYGNavMesh BuildGridMesh()
        {
            const int width = 4;
            const int depth = 4;
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

        private static CYGPlayerInput CreateLocalInput(int tick)
        {
            return new CYGPlayerInput(tick, (byte)(tick % 17 == 0 ? CYGPlayerInput.ButtonJump : 0), 1000, 0);
        }

        private static CYGPlayerInput CreateRemoteInput(int tick)
        {
            return new CYGPlayerInput(
                tick,
                0,
                (short)(300 + tick * 10),
                (short)(-200 - tick * 5));
        }

        private static uint Checksum<T>(T value)
            where T : struct, ICYGValue
        {
            uint crc = 0xFFFFFFFFu;
            value.ComputeChecksum(ref crc);
            return crc ^ 0xFFFFFFFFu;
        }

        private static void Require(bool condition, string message)
        {
            _assertions++;
            if (!condition)
            {
                throw new InvalidOperationException(message);
            }
        }

        private static void Equal<T>(T expected, T actual, string message)
        {
            _assertions++;
            if (!Equals(expected, actual))
            {
                throw new InvalidOperationException(
                    message + " (expected " + expected + ", got " + actual + ")");
            }
        }
    }
}
