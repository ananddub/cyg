using System;

namespace CookYourGame.Sim.Tests
{
    internal static class FeatureTests
    {
        private static int _assertions;

        public static int AssertionCount
        {
            get { return _assertions; }
        }

        public static void RunAll()
        {
            TestTimerExpiry();
            TestTimerLoopAndStop();
            TestTimerComponentCodec();
            TestCountdownSystem();
            TestAssetRegistry();
            TestStaticMeshRaycast();
            TestRampMesh();
            TestStaticMeshCastsAndOverlap();
            TestCharacterLandsOnMesh();
            TestMeshMovementSystem();
            TestPhysicsMeshRigidBodies();
            TestPhysics3DSystemMeshBody();
            TestAttackFlagFromInput();
            TestMultiClientMatch();
        }

        private static void TestTimerExpiry()
        {
            CYGCountdownTimer timer = CYGCountdownTimer.Create(FP.FromInt(2));
            Require(timer.IsRunning, "timer starts running");
            Equal(FP.FromInt(2), timer.Remaining, "timer starts full");
            Equal(FP.Zero, timer.Progress, "timer starts at zero progress");

            Require(!timer.Tick(FP.One), "timer does not expire before the duration");
            Require(timer.IsRunning, "timer still running halfway");
            Equal(FP.FromInt(1), timer.Remaining, "timer remaining after one second");

            Require(timer.Tick(FP.One), "timer expires on the duration tick");
            Require(!timer.IsRunning, "expired timer stops");
            Require(timer.HasExpired, "expired flag is set");
            Equal(FP.Zero, timer.Remaining, "expired timer clamps to zero");
            Equal(FP.One, timer.Progress, "expired progress is one");
            Require(!timer.Tick(FP.One), "expired timer does not fire again");
        }

        private static void TestTimerLoopAndStop()
        {
            CYGCountdownTimer timer = CYGCountdownTimer.Create(FP.One, loop: true);
            Require(timer.Tick(FP.FromFraction(5, 2)), "looping timer reports the wrap");
            Require(timer.IsRunning, "looping timer keeps running after wrap");
            Equal(FP.Half, timer.Remaining, "looping timer wraps the remainder");

            timer.Stop();
            Require(!timer.IsRunning, "stop clears running");
            Require(!timer.Tick(FP.One), "stopped timer does not tick");

            timer.Reset();
            Equal(FP.One, timer.Remaining, "reset restores the duration");
            Require(!timer.HasExpired, "reset clears the expired flag");
            timer.Start();
            Require(timer.IsRunning, "start after reset runs again");
        }

        private static void TestTimerComponentCodec()
        {
            CYGCountdown countdown = CYGCountdown.Create(FP.FromInt(3), loop: true);
            countdown.Timer.Tick(FP.One);

            byte[] buffer = new byte[CYGCountdown.SizeInBytes];
            Equal(CYGCountdown.SizeInBytes, countdown.WriteTo(buffer, 0), "countdown write size");

            CYGCountdown read = default(CYGCountdown);
            read.ReadFrom(buffer, 0);
            Equal(countdown.Timer.Duration, read.Timer.Duration, "countdown duration roundtrip");
            Equal(countdown.Timer.Remaining, read.Timer.Remaining, "countdown remaining roundtrip");
            Equal(countdown.Timer.Flags, read.Timer.Flags, "countdown flags roundtrip");

            uint before = 0;
            countdown.ComputeChecksum(ref before);
            read.Timer.Remaining = FP.One;
            uint after = 0;
            read.ComputeChecksum(ref after);
            Require(before != after, "countdown checksum includes remaining time");
            Equal(-1031, countdown.ComponentTypeId, "countdown type id");
        }

        private static void TestCountdownSystem()
        {
            CYGWorld world = new CYGWorld();
            world.Register<CYGCountdown>();
            CYGEntityRef entity = world.CreateEntity();
            world.Set(entity, CYGCountdown.Create(FP.One));

            CYGCountdownSystem system = new CYGCountdownSystem();
            int expired = 0;
            system.OnExpired += (_, __) => expired++;

            CYGSystemRunner systems = new CYGSystemRunner(world, CYGDeterminismConfig.Default);
            systems.AddSystem(system);
            systems.Start();

            for (int tick = 0; tick < 65; tick++)
            {
                systems.Tick(tick, false, null, null, 0);
            }

            Require(world.TryGet(entity, out CYGCountdown countdown), "countdown component still exists");
            Require(countdown.Timer.HasExpired, "countdown system expires the component");
            Equal(1, expired, "countdown expiry event fires once");
            Equal(FP.Zero, countdown.Timer.Remaining, "countdown system clamps remaining time");
        }

        private sealed class SampleStats
        {
            public int Health;
        }

        private static void TestAssetRegistry()
        {
            CYGAssetRegistry registry = new CYGAssetRegistry();
            SampleStats hero = new SampleStats { Health = 150 };

            int id = registry.Register("hero_stats", hero);
            Require(id != 0, "asset key derives a stable id");
            Equal(id, CYGAssetRegistry.StableId("hero_stats"), "stable id is deterministic");

            SampleStats byId;
            Require(registry.TryGet(id, out byId), "asset resolves by id");
            Equal(150, byId.Health, "asset payload by id");

            SampleStats byKey;
            Require(registry.TryGet("hero_stats", out byKey), "asset resolves by key");
            Require(ReferenceEquals(hero, byKey), "same asset instance by key");

            Require(registry.Register(77, hero, "hero_alias"), "explicit id registers");
            Require(registry.Contains(77), "explicit id is present");
            Require(registry.Remove(77), "asset removes by id");
            Require(!registry.Contains(77), "removed asset is gone");
            CYGAssetId assetId = new CYGAssetId(id);
            Require(registry.TryGet(assetId, out byId), "asset resolves by CYGAssetId");
        }

        private static void TestStaticMeshRaycast()
        {
            CYGStaticMesh mesh = FloorMesh();
            Equal(2, mesh.TriangleCount, "floor mesh triangle count");

            FPRay ray = new FPRay(
                new FPVector3(FP.Zero, FP.FromInt(2), FP.Zero),
                FPVector3.Down);
            Require(mesh.Raycast(ray, FP.FromInt(5), out FPRaycastHit hit), "ray hits the mesh floor");
            Near(FP.FromInt(2), hit.Distance, "ray distance to floor");
            Require(hit.Normal.y > FP.Zero, "floor hit normal points up");

            FPRay miss = new FPRay(
                new FPVector3(FP.FromInt(20), FP.FromInt(2), FP.Zero),
                FPVector3.Down);
            Require(!mesh.Raycast(miss, FP.FromInt(5), out _), "ray outside the mesh misses");
        }

        private static void TestRampMesh()
        {
            FPVector3[] vertices =
            {
                new FPVector3(FP.FromInt(-2), FP.Zero, FP.Zero),
                new FPVector3(FP.FromInt(2), FP.Zero, FP.Zero),
                new FPVector3(FP.FromInt(2), FP.FromInt(2), FP.FromInt(4)),
                new FPVector3(FP.FromInt(-2), FP.FromInt(2), FP.FromInt(4)),
            };
            CYGStaticMesh ramp = new CYGStaticMesh(5150, "Ramp", vertices, new[] { 0, 1, 2, 0, 2, 3 });

            FPRay ray = new FPRay(
                new FPVector3(FP.Zero, FP.FromInt(3), FP.FromInt(2)),
                FPVector3.Down);
            Require(ramp.Raycast(ray, FP.FromInt(5), out FPRaycastHit hit), "ray hits the ramp");
            Near(FP.FromInt(2), hit.Distance, "ramp ray distance");
            Near(FP.One, hit.Point.y, "ramp hit height follows the slope");
            Require(hit.Normal.y > FP.Zero, "ramp normal points up");
        }

        private static void TestStaticMeshCastsAndOverlap()
        {
            CYGStaticMesh mesh = FloorMesh();

            FPSphere sphere = new FPSphere(new FPVector3(FP.Zero, FP.FromInt(2), FP.Zero), FP.Half);
            Require(mesh.SphereCast(sphere, FPVector3.Down, FP.FromInt(4), out FPRaycastHit sphereHit),
                "sphere cast hits the mesh floor");
            Near(FP.FromFraction(3, 2), sphereHit.Distance, "sphere cast distance accounts for the radius");

            FPCapsule capsule = FPCapsule.CreateVertical(
                new FPVector3(FP.Zero, FP.FromInt(1), FP.Zero),
                FP.FromInt(2),
                FP.Half);
            Require(mesh.CapsuleCast(capsule, FPVector3.Down, FP.FromInt(2), out FPRaycastHit capsuleHit),
                "capsule cast hits the mesh floor");
            Require(capsuleHit.Distance <= FP.Half, "capsule cast touches the floor near the capsule bottom");

            FPCapsule touching = FPCapsule.CreateVertical(
                new FPVector3(FP.Zero, FP.FromFraction(1, 2), FP.Zero),
                FP.FromInt(2),
                FP.Half);
            Require(mesh.Overlaps(touching), "capsule overlap detects the mesh floor");
        }

        private static void TestCharacterLandsOnMesh()
        {
            SimpleCollisionWorld world = new SimpleCollisionWorld();
            world.AddMesh(FloorMesh());

            FPCharacterController controller = new FPCharacterController(
                new FPVector3(FP.Zero, FP.FromInt(3), FP.Zero),
                FP.Half,
                FP.FromInt(2));
            controller.Move(FPVector3.Down * FP.FromInt(4), world);

            Require(controller.IsGrounded, "character lands on a baked mesh");
            Require(controller.Position.y < FP.FromFraction(1, 5), "character settles on the mesh surface");
        }

        private static void TestMeshMovementSystem()
        {
            CYGWorld world = new CYGWorld();
            world.Register<CYGSimAgent>();

            CYGEntityRef entity = world.CreateEntity();
            CYGSimAgent agent = CYGSimAgent.Create(1, new FPVector3(FP.Zero, FP.FromInt(3), FP.Zero));
            agent.Grounded = false;
            world.Set(entity, agent);

            SimpleCollisionWorld collision = new SimpleCollisionWorld();
            collision.AddMesh(FloorMesh());

            CYGDeterminismConfig config = CYGDeterminismConfig.Default;
            CYGSystemRunner systems = new CYGSystemRunner(world, config);
            CYGCharacterMovementSystem movement = new CYGCharacterMovementSystem
            {
                CollisionWorld = collision,
            };
            systems.AddSystem(movement);
            systems.Start();

            for (int tick = 0; tick < 180; tick++)
            {
                systems.Tick(tick, false, null, null, 0);
            }

            Require(world.TryGet(entity, out agent), "mesh movement agent exists");
            Require(agent.Grounded, "mesh movement system lands on the baked floor");
            Require(agent.Position.y >= FP.Zero && agent.Position.y < FP.FromFraction(1, 10),
                "mesh movement settles on the floor height: " + agent.Position.y);
            Require(agent.Position.y > -FP.One, "mesh movement does not fall through the floor");
        }

        private static void TestPhysicsMeshRigidBodies()
        {
            CYGPhysicsSettings settings = new CYGPhysicsSettings
            {
                EnableSleeping = false,
                EnableCcd = true,
            };
            CYGPhysicsWorld physics = new CYGPhysicsWorld(settings);
            int meshBodyId = physics.AddStaticMesh(FloorMesh());
            Require(meshBodyId >= 0, "static mesh body added");
            Equal(1, physics.StaticMeshBodyCount, "static mesh body count");

            CYGPhysicsBody sphere = CYGPhysicsBody.CreateDynamic(CYGCollider.Sphere(FP.Half), FP.One);
            sphere.Position = new FPVector3(FP.Zero, FP.FromInt(3), FP.Zero);
            int sphereId = physics.AddBody(sphere);

            for (int i = 0; i < 240; i++)
                physics.Step();

            FP sphereY = physics.GetBody(sphereId).Position.y;
            Require(sphereY > FP.FromFraction(45, 100) && sphereY < FP.FromFraction(55, 100),
                "dynamic sphere rests on the mesh floor: " + sphereY);

            FPRay down = new FPRay(new FPVector3(FP.FromInt(3), FP.FromInt(3), FP.FromInt(3)), FPVector3.Down);
            CYGPhysicsHit hit = physics.Raycast(down, FP.FromInt(5));
            Require(hit.Hit, "physics raycast hits the mesh body");
            Equal(meshBodyId, hit.BodyId, "physics raycast returns the mesh body id");

            FPSphere castSphere = new FPSphere(
                new FPVector3(FP.FromInt(2), FP.FromInt(2), FP.FromInt(2)),
                FP.Half);
            CYGPhysicsHit castHit = physics.SphereCast(castSphere, FPVector3.Down, FP.FromInt(4));
            Require(castHit.Hit, "physics sphere cast hits the mesh body");
            Equal(meshBodyId, castHit.BodyId, "sphere cast returns the mesh body id");

            CYGPhysicsBody box = CYGPhysicsBody.CreateDynamic(
                CYGCollider.Box(new FPVector3(FP.Half, FP.Half, FP.Half)), FP.One);
            box.Position = new FPVector3(FP.FromInt(-2), FP.FromInt(3), FP.Zero);
            int boxId = physics.AddBody(box);

            CYGPhysicsBody capsule = CYGPhysicsBody.CreateDynamic(
                CYGCollider.Capsule(FP.Half, FP.FromInt(2)), FP.One);
            capsule.Position = new FPVector3(FP.FromInt(2), FP.FromInt(3), FP.Zero);
            int capsuleId = physics.AddBody(capsule);

            for (int i = 0; i < 300; i++)
                physics.Step();

            FP boxY = physics.GetBody(boxId).Position.y;
            Require(boxY > FP.FromFraction(45, 100) && boxY < FP.FromFraction(55, 100),
                "dynamic box rests on the mesh floor: " + boxY);

            FP capsuleY = physics.GetBody(capsuleId).Position.y;
            Require(capsuleY > FP.FromFraction(95, 100) && capsuleY < FP.FromFraction(105, 100),
                "dynamic capsule rests on the mesh floor: " + capsuleY);
        }

        private static void TestPhysics3DSystemMeshBody()
        {
            CYGWorld world = new CYGWorld();
            world.Register<CYGPhysicsBody3D>();

            CYGPhysics3DSystem physics = new CYGPhysics3DSystem(new CYGPhysicsSettings
            {
                EnableSleeping = false,
            });
            Equal(0, physics.Physics.StaticMeshBodyCount, "system starts without mesh bodies");
            physics.AddStaticMesh(FloorMesh());
            Equal(1, physics.Physics.StaticMeshBodyCount, "system registers the baked mesh");

            CYGEntityRef entity = world.CreateEntity();
            CYGPhysicsBody3D body = new CYGPhysicsBody3D
            {
                BodyId = -1,
                BodyType = (byte)CYGBodyType.Dynamic,
                ShapeType = (byte)CYGShapeType.Sphere,
                Flags = CYGPhysicsBody3D.ActiveFlag | CYGPhysicsBody3D.AwakeFlag | CYGPhysicsBody3D.CcdFlag,
                Position = new FPVector3(FP.FromInt(2), FP.FromInt(3), FP.FromInt(2)),
                Rotation = FPQuaternion.Identity,
                Radius = FP.Half,
                HalfExtents = new FPVector3(FP.Half, FP.Half, FP.Half),
                Mass = FP.One,
                Friction = FP.Half,
                Restitution = FP.Zero,
                GravityScale = FP.One,
                Layer = 0,
                LayerMask = 0xFFFFFFFFu,
            };
            world.Set(entity, body);

            CYGSystemRunner systems = new CYGSystemRunner(world, CYGDeterminismConfig.Default);
            systems.AddSystem(physics);
            systems.Start();
            for (int tick = 0; tick < 240; tick++)
                systems.Tick(tick, false, null, null, 0);

            Require(world.TryGet(entity, out body), "physics body entity exists");
            Require(body.Position.y > FP.FromFraction(45, 100) && body.Position.y < FP.FromFraction(55, 100),
                "Physics3D ECS sphere rests on the registered mesh: " + body.Position.y);
            Equal(1, physics.Physics.StaticMeshBodyCount, "mesh body survives ECS synchronization");
        }

        private static void TestAttackFlagFromInput()
        {
            CYGWorld world = new CYGWorld();
            world.Register<CYGSimAgent>();

            CYGEntityRef entity = world.CreateEntity();
            world.Set(entity, CYGSimAgent.Create(7, FPVector3.Zero));

            CYGSystemRunner systems = new CYGSystemRunner(world, CYGDeterminismConfig.Default);
            systems.AddSystem(new CYGCharacterMovementSystem());
            systems.Start();

            CYGPlayerInput attack = new CYGPlayerInput(1, CYGPlayerInput.ButtonAction1, 0, 0);
            systems.Tick(1, false, new[] { attack }, new[] { 7 }, 1);

            CYGSimAgent agent;
            Require(world.TryGet(entity, out agent), "attack agent exists");
            Require(agent.Attacking, "holding Action1 sets the attacking flag");

            CYGPlayerInput idle = new CYGPlayerInput(2, 0, 0, 0);
            systems.Tick(2, false, new[] { idle }, new[] { 7 }, 1);
            Require(world.TryGet(entity, out agent), "attack agent still exists");
            Require(!agent.Attacking, "releasing Action1 clears the attacking flag");
        }

        private static void TestMultiClientMatch()        {
            CYGMultiClientMatch match = new CYGMultiClientMatch(new CYGMultiClientOptions
            {
                ClientCount = 3,
                PlayersPerClient = 2,
                Ticks = 120,
                Seed = 77,
            });

            match.RunToEnd();
            Require(match.IsComplete, "multi-client match completes");
            Require(!match.Diverged, "multi-client checksums agree: " + match.Divergence);

            uint[] checksums = match.GetChecksums();
            Equal(3, checksums.Length, "one checksum per client");
            Equal(checksums[0], checksums[1], "client 0 and 1 checksums match");
            Equal(checksums[0], checksums[2], "client 0 and 2 checksums match");
        }

        /// <summary>Two triangles forming a 10x10 floor at y=0.</summary>
        private static CYGStaticMesh FloorMesh()
        {
            FPVector3[] vertices =
            {
                new FPVector3(FP.FromInt(-5), FP.Zero, FP.FromInt(-5)),
                new FPVector3(FP.FromInt(5), FP.Zero, FP.FromInt(-5)),
                new FPVector3(FP.FromInt(5), FP.Zero, FP.FromInt(5)),
                new FPVector3(FP.FromInt(-5), FP.Zero, FP.FromInt(5)),
            };
            return new CYGStaticMesh(4242, "TestFloor", vertices, new[] { 0, 2, 1, 0, 3, 2 });
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
            Require(Equals(expected, actual), message + " (expected " + expected + ", got " + actual + ")");
        }

        private static void Near(FP expected, FP actual, string message)
        {
            FP tolerance = FP.FromFraction(1, 1000);
            Require(FP.Abs(expected - actual) <= tolerance,
                message + " (expected " + expected + ", got " + actual + ")");
        }
    }
}
