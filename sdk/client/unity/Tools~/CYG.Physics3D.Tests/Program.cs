using System;
using System.Collections.Generic;

namespace CookYourGame.Sim.Tests
{
    internal static class Program
    {
        private static int _assertions;

        private static readonly FP Half = FP.FromFraction(1, 2);
        private static readonly FP Quarter = FP.FromFraction(1, 4);

        private static int Main()
        {
            try
            {
                Console.WriteLine("CYG physics3d tests");
                Run("sphere-resting", TestSphereRestingOnGround);
                Run("sphere-momentum", TestSphereMomentumExchange);
                Run("friction", TestFrictionSlowsSlidingBox);
                Run("box-stack", TestBoxStackStable);
                Run("oriented-narrowphase", TestOrientedNarrowphase);
                Run("oriented-raycast", TestOrientedRaycast);
                Run("oriented-sphere-rest", TestOrientedSphereRest);
                Run("oriented-box-rest", TestOrientedBoxRest);
                Run("oriented-mesh-rest", TestOrientedMeshRest);
                Run("oriented-box-stack", TestOrientedBoxStack);
                Run("oriented-capsule-overlap", TestOrientedCapsuleOverlap);
                Run("oriented-capsule-rest", TestOrientedCapsuleRest);
                Run("rolling-friction", TestRollingFriction);
                Run("inertia-tensor", TestInertiaTensor);
                Run("queries", TestQueries);
                Run("triggers", TestTriggerEvents);
                Run("narrowphase-pairs", TestNarrowphasePairCoverage);
                Run("distance-joint", TestDistanceJoint);
                Run("spring-joint", TestSpringJoint);
                Run("joint-torque", TestJointTorque);
                Run("ccd", TestCcdPreventsTunneling);
                Run("collision-world", TestCollisionWorldInterface);
                Run("determinism-snapshot", TestDeterminismAndSnapshot);
                Run("allocation", TestFixedStepAllocation);

                Console.WriteLine("CYG physics3d tests passed: assertions=" + _assertions);
                return 0;
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine("CYG physics3d tests FAILED: " + ex.GetType().Name + ": " + ex.Message);
                Console.Error.WriteLine(ex.StackTrace);
                return 1;
            }
        }

        #region Harness

        private static void Run(string name, Action test)
        {
            test();
            Console.WriteLine("  ok " + name);
        }

        private static void Require(bool condition, string message)
        {
            _assertions++;
            if (!condition)
                throw new Exception(message);
        }

        private static void RequireNear(FP actual, FP expected, FP tolerance, string message)
        {
            _assertions++;
            FP difference = FP.Abs(actual - expected);
            if (difference > tolerance)
            {
                throw new Exception(message + " (actual=" + actual + ", expected=" + expected +
                                    ", error=" + difference + ", tolerance=" + tolerance + ")");
            }
        }

        private static void RequireThrows<T>(Action action, string message) where T : Exception
        {
            _assertions++;
            try
            {
                action();
            }
            catch (T)
            {
                return;
            }
            catch (Exception ex)
            {
                throw new Exception(message + " (threw " + ex.GetType().Name + " instead of " + typeof(T).Name + ")");
            }

            throw new Exception(message + " (no exception was thrown)");
        }

        private static FP Frac(int numerator, int denominator)
        {
            return FP.FromFraction(numerator, denominator);
        }

        private static FPVector3 V(int x, int y, int z)
        {
            return new FPVector3(x, y, z);
        }

        private static FPVector3 V(FP x, FP y, FP z)
        {
            return new FPVector3(x, y, z);
        }

        private static CYGPhysicsWorld NewWorld(bool useGravity)
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld();
            if (!useGravity)
                world.Settings.Gravity = FPVector3.Zero;
            return world;
        }

        private static int AddGround(CYGPhysicsWorld world, FP friction)
        {
            return AddStaticBox(world, V(0, -Half, 0), V(20, Half, 20), friction);
        }

        private static int AddStaticBox(CYGPhysicsWorld world, FPVector3 position, FPVector3 halfExtents, FP friction)
        {
            CYGPhysicsBody body = CYGPhysicsBody.CreateStatic(CYGCollider.Box(halfExtents));
            body.Position = position;
            body.Friction = friction;
            return world.AddBody(body);
        }

        private static int AddStaticSphere(CYGPhysicsWorld world, FPVector3 position, FP radius)
        {
            CYGPhysicsBody body = CYGPhysicsBody.CreateStatic(CYGCollider.Sphere(radius));
            body.Position = position;
            return world.AddBody(body);
        }

        private static int AddDynamicBox(
            CYGPhysicsWorld world,
            FPVector3 position,
            FPVector3 halfExtents,
            FP mass,
            FP friction,
            FP restitution)
        {
            CYGPhysicsBody body = CYGPhysicsBody.CreateDynamic(CYGCollider.Box(halfExtents), mass);
            body.Position = position;
            body.Friction = friction;
            body.Restitution = restitution;
            return world.AddBody(body);
        }

        private static int AddDynamicSphere(
            CYGPhysicsWorld world,
            FPVector3 position,
            FP radius,
            FP mass,
            FP friction,
            FP restitution)
        {
            CYGPhysicsBody body = CYGPhysicsBody.CreateDynamic(CYGCollider.Sphere(radius), mass);
            body.Position = position;
            body.Friction = friction;
            body.Restitution = restitution;
            return world.AddBody(body);
        }

        private static int AddDynamicCapsule(
            CYGPhysicsWorld world,
            FPVector3 position,
            FP radius,
            FP height,
            FP mass)
        {
            CYGPhysicsBody body = CYGPhysicsBody.CreateDynamic(CYGCollider.Capsule(radius, height), mass);
            body.Position = position;
            body.Friction = Frac(1, 2);
            return world.AddBody(body);
        }

        #endregion

        #region Resting contact and momentum

        private static void TestSphereRestingOnGround()
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld();
            AddGround(world, Frac(4, 5));
            int sphereId = AddDynamicSphere(world, V(0, 3, 0), Half, 1, Frac(1, 2), FP.Zero);
            CYGPhysicsBody sphere = world.GetBody(sphereId);

            FP minY = FP.MaxValue;
            FP lastMin = FP.MaxValue;
            FP lastMax = FP.MinValue;

            for (int step = 0; step < 300; step++)
            {
                world.Step();
                FP y = sphere.Position.y;
                if (y < minY)
                    minY = y;
                if (step >= 270)
                {
                    if (y < lastMin) lastMin = y;
                    if (y > lastMax) lastMax = y;
                }
            }

            RequireNear(sphere.Position.y, Half, Frac(1, 50), "sphere must rest on the ground surface");
            Require(sphere.LinearVelocity.Magnitude < Frac(1, 100), "resting sphere velocity must be near zero");
            Require(minY > Frac(45, 100), "sphere must not sink through the ground (minY=" + minY + ")");
            Require(lastMax - lastMin <= Frac(1, 1000), "resting sphere must not jitter");
            Require(sphere.Position.x == FP.Zero && sphere.Position.z == FP.Zero, "resting sphere must not drift");
            Require(!sphere.IsAwake, "resting sphere should enter the deterministic sleep state");
        }

        private static void TestSphereMomentumExchange()
        {
            CYGPhysicsWorld inelastic = NewWorld(false);
            int inelasticA = AddDynamicSphere(inelastic, V(-2, 0, 0), Half, 1, Frac(1, 2), FP.Zero);
            int inelasticB = AddDynamicSphere(inelastic, V(2, 0, 0), Half, 1, Frac(1, 2), FP.Zero);
            inelastic.GetBody(inelasticA).LinearVelocity = V(3, 0, 0);
            inelastic.GetBody(inelasticB).LinearVelocity = V(-3, 0, 0);

            for (int step = 0; step < 90; step++)
                inelastic.Step();

            CYGPhysicsBody bodyA = inelastic.GetBody(inelasticA);
            CYGPhysicsBody bodyB = inelastic.GetBody(inelasticB);
            Require(FP.Abs(bodyA.LinearVelocity.x) < Frac(1, 20), "inelastic sphere A must stop");
            Require(FP.Abs(bodyB.LinearVelocity.x) < Frac(1, 20), "inelastic sphere B must stop");
            RequireNear(bodyA.LinearVelocity.x + bodyB.LinearVelocity.x, FP.Zero, Frac(1, 20),
                "equal-mass collision must conserve momentum");

            CYGPhysicsWorld elastic = NewWorld(false);
            int elasticA = AddDynamicSphere(elastic, V(-2, 0, 0), Half, 1, FP.Zero, FP.One);
            int elasticB = AddDynamicSphere(elastic, V(2, 0, 0), Half, 1, FP.Zero, FP.One);
            elastic.GetBody(elasticA).LinearVelocity = V(3, 0, 0);
            elastic.GetBody(elasticB).LinearVelocity = V(-3, 0, 0);

            for (int step = 0; step < 90; step++)
                elastic.Step();

            CYGPhysicsBody elasticBodyA = elastic.GetBody(elasticA);
            CYGPhysicsBody elasticBodyB = elastic.GetBody(elasticB);
            RequireNear(elasticBodyA.LinearVelocity.x, -3, Frac(1, 5), "elastic collision must reverse sphere A");
            RequireNear(elasticBodyB.LinearVelocity.x, 3, Frac(1, 5), "elastic collision must reverse sphere B");
            Require(elasticBodyB.Position.x - elasticBodyA.Position.x > FP.One,
                "elastic spheres must separate after the bounce");
        }

        private static void TestFrictionSlowsSlidingBox()
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld();
            AddGround(world, FP.One);
            int boxId = AddDynamicBox(world, V(0, Half, 0), V(Half, Half, Half), 1, FP.One, FP.Zero);
            world.GetBody(boxId).LinearVelocity = V(5, 0, 0);

            for (int step = 0; step < 30; step++)
                world.Step();

            CYGPhysicsBody box = world.GetBody(boxId);
            Require(box.LinearVelocity.x >= FP.Zero, "Coulomb friction must not reverse the sliding box");
            Require(box.LinearVelocity.x < Frac(9, 2), "friction must noticeably slow the sliding box");
            Require(box.Position.x > FP.Zero && box.Position.x < FP.FromInt(2),
                "friction sliding distance out of range (x=" + box.Position.x + ")");
        }

        private static void TestBoxStackStable()
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld();
            AddGround(world, Frac(4, 5));

            int[] boxes = new int[3];
            for (int i = 0; i < 3; i++)
            {
                FP y = Half + FP.FromInt(i);
                boxes[i] = AddDynamicBox(world, V(FP.Zero, y, FP.Zero), V(Half, Half, Half), 1,
                    Frac(7, 10), FP.Zero);
            }

            FP[][] lastPositions = new FP[3][];
            for (int i = 0; i < 3; i++)
                lastPositions[i] = new FP[2];

            for (int step = 0; step < 300; step++)
            {
                world.Step();
                if (step == 270)
                {
                    for (int i = 0; i < 3; i++)
                    {
                        FP y = world.GetBody(boxes[i]).Position.y;
                        lastPositions[i][0] = y;
                        lastPositions[i][1] = y;
                    }
                }
                else if (step > 270)
                {
                    for (int i = 0; i < 3; i++)
                    {
                        FP y = world.GetBody(boxes[i]).Position.y;
                        if (y < lastPositions[i][0]) lastPositions[i][0] = y;
                        if (y > lastPositions[i][1]) lastPositions[i][1] = y;
                    }
                }
            }

            for (int i = 0; i < 3; i++)
            {
                CYGPhysicsBody box = world.GetBody(boxes[i]);
                RequireNear(box.Position.y, Half + FP.FromInt(i), Frac(1, 30),
                    "stacked box " + i + " height");
                Require(FP.Abs(box.Position.x) <= Frac(1, 50), "stacked box " + i + " must not drift in X");
                Require(FP.Abs(box.Position.z) <= Frac(1, 50), "stacked box " + i + " must not drift in Z");
                Require(lastPositions[i][1] - lastPositions[i][0] <= Frac(1, 500),
                    "stacked box " + i + " must not jitter at rest");
            }
        }

        #endregion

        #region Oriented collision and rotational response

        private static void TestOrientedNarrowphase()
        {
            CYGPhysicsBody a = CYGPhysicsBody.CreateStatic(CYGCollider.Box(V(Half, Half, Half)));
            a.Position = FPVector3.Zero;
            a.Rotation = FPQuaternion.AngleAxis(FP.FromInt(45), FPVector3.Up);

            CYGPhysicsBody b = CYGPhysicsBody.CreateDynamic(CYGCollider.Box(V(Half, Half, Half)), FP.One);
            b.Position = V(Frac(4, 5), FP.Zero, FP.Zero);
            b.Rotation = FPQuaternion.AngleAxis(FP.FromInt(30), FPVector3.Forward);
            Require(CYGPhysicsNarrowphase.Overlaps(a, b), "rotated boxes must overlap");

            b.Position = V(3, FP.Zero, FP.Zero);
            Require(!CYGPhysicsNarrowphase.Overlaps(a, b), "separated rotated boxes must not overlap");
        }

        private static void TestOrientedRaycast()
        {
            CYGPhysicsWorld world = NewWorld(false);
            int id = AddStaticBox(world, FPVector3.Zero, V(Half, Half, Half), FP.One);
            world.GetBody(id).Rotation = FPQuaternion.AngleAxis(FP.FromInt(45), FPVector3.Up);

            CYGPhysicsHit rayHit = world.Raycast(new FPRay(V(3, 0, 0), V(-1, 0, 0)), FP.FromInt(10));
            Require(rayHit.Hit, "ray must hit the rotated box");
            // A unit cube yawed 45 degrees reaches sqrt(2)/2 along X.
            RequireNear(rayHit.Distance, FP.FromInt(3) - FP.Sqrt(FP.Half), Frac(1, 50), "rotated box ray distance");

            CYGPhysicsHit sphereHit = world.SphereCast(
                new FPSphere(V(3, 0, 0), Half),
                V(-1, 0, 0),
                FP.FromInt(10));
            Require(sphereHit.Hit, "sphere cast must hit the rotated box");
            RequireNear(sphereHit.Distance, FP.FromInt(3) - FP.Sqrt(FP.Half) - Half, Frac(1, 50), "rotated box sphere-cast distance");

            FPCapsule capsule = new FPCapsule(V(3, -Quarter, 0), V(3, Quarter, 0), Frac(1, 5));
            CYGPhysicsHit capsuleHit = world.CapsuleCast(capsule, V(-1, 0, 0), FP.FromInt(10));
            Require(capsuleHit.Hit, "capsule cast must hit the rotated box");
        }

        private static void TestOrientedSphereRest()
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld();
            int boxId = AddStaticBox(world, FPVector3.Zero, V(Half, Half, Half), Frac(4, 5));
            world.GetBody(boxId).Rotation = FPQuaternion.AngleAxis(FP.FromInt(45), FPVector3.Up);
            int sphereId = AddDynamicSphere(world, V(0, 2, 0), Half, FP.One, Frac(1, 2), FP.Zero);

            for (int step = 0; step < 300; step++)
                world.Step();

            CYGPhysicsBody sphere = world.GetBody(sphereId);
            RequireNear(sphere.Position.y, FP.One, Frac(1, 30), "sphere on rotated box height");
            Require(FP.Abs(sphere.Position.x) < Frac(1, 20), "sphere must not slide off rotated box X");
            Require(FP.Abs(sphere.Position.z) < Frac(1, 20), "sphere must not slide off rotated box Z");
        }

        private static void TestOrientedBoxRest()
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld();
            AddGround(world, Frac(4, 5));
            int boxId = AddDynamicBox(world, V(0, 2, 0), V(Half, Half, Half), FP.One, Frac(7, 10), FP.Zero);
            world.GetBody(boxId).Rotation = FPQuaternion.AngleAxis(FP.FromInt(30), FPVector3.Up);

            for (int step = 0; step < 300; step++)
                world.Step();

            CYGPhysicsBody box = world.GetBody(boxId);
            RequireNear(box.Position.y, Half, Frac(1, 30), "yawed box must rest on the ground");
            Require(FP.Abs(box.Position.x) < Frac(1, 40), "yawed box must not drift in X");
            Require(FP.Abs(box.Position.z) < Frac(1, 40), "yawed box must not drift in Z");
            Require(!box.IsAwake, "yawed resting box should sleep");
        }

        private static void TestOrientedMeshRest()
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld(new CYGPhysicsSettings
            {
                EnableSleeping = false,
                EnableCcd = true,
            });
            FPVector3[] vertices =
            {
                new FPVector3(-FP.FromInt(5), FP.Zero, -FP.FromInt(5)),
                new FPVector3(FP.FromInt(5), FP.Zero, -FP.FromInt(5)),
                new FPVector3(FP.FromInt(5), FP.Zero, FP.FromInt(5)),
                new FPVector3(-FP.FromInt(5), FP.Zero, FP.FromInt(5)),
            };
            CYGStaticMesh mesh = new CYGStaticMesh(9301, "OrientedMeshFloor", vertices, new[] { 0, 2, 1, 0, 3, 2 });
            world.AddStaticMesh(mesh);

            int boxId = AddDynamicBox(world, V(-2, 2, 0), V(Half, Half, Half), FP.One, Frac(7, 10), FP.Zero);
            world.GetBody(boxId).Rotation = FPQuaternion.AngleAxis(FP.FromInt(30), FPVector3.Up);

            for (int step = 0; step < 300; step++)
                world.Step();

            CYGPhysicsBody box = world.GetBody(boxId);
            RequireNear(box.Position.y, Half, Frac(1, 20), "yawed box on mesh height");
            Require(FP.Abs(box.Position.x + FP.FromInt(2)) < Frac(1, 10), "yawed box on mesh must not slide");
        }

        private static void TestOrientedBoxStack()
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld();
            int baseId = AddStaticBox(world, FPVector3.Zero, V(Half, Half, Half), Frac(4, 5));
            world.GetBody(baseId).Rotation = FPQuaternion.AngleAxis(FP.FromInt(30), FPVector3.Up);

            int boxId = AddDynamicBox(world, V(0, 2, 0), V(Half, Half, Half), FP.One, Frac(7, 10), FP.Zero);
            world.GetBody(boxId).Rotation = FPQuaternion.AngleAxis(FP.FromInt(10), FPVector3.Up);

            for (int step = 0; step < 300; step++)
                world.Step();

            CYGPhysicsBody box = world.GetBody(boxId);
            RequireNear(box.Position.y, FP.One, Frac(1, 20), "yawed box stack height");
            Require(FP.Abs(box.Position.x) < Frac(1, 20), "yawed box stack must not drift in X");
            Require(FP.Abs(box.Position.z) < Frac(1, 20), "yawed box stack must not drift in Z");
        }

        private static void TestInertiaTensor()
        {
            CYGPhysicsBody body = CYGPhysicsBody.CreateDynamic(
                CYGCollider.Box(new FPVector3(Frac(1, 4), FP.One, Frac(1, 4))),
                FP.One);
            Require(body.InertiaLocal.y < body.InertiaLocal.x, "tall box must have a smaller Y inertia");
            Require(body.InverseInertiaLocal.x != body.InverseInertiaLocal.y, "inertia tensor must be anisotropic");

            body.ApplyImpulseAtPoint(V(0, 0, 1), new FPVector3(Frac(1, 4), FP.One, 0));
            Require(body.AngularVelocity.SqrMagnitude > FP.Zero, "off-centre impulse must spin the body");
            Require(FP.Abs(body.AngularVelocity.y) > FP.Abs(body.AngularVelocity.x),
                "inertia tensor must couple the off-centre impulse into the small axis");
        }

        private static void TestOrientedCapsuleOverlap()
        {
            CYGPhysicsWorld world = NewWorld(false);
            int id = AddStaticBox(world, FPVector3.Zero, V(Half, Half, Half), FP.One);
            world.GetBody(id).Rotation = FPQuaternion.AngleAxis(FP.FromInt(45), FPVector3.Forward);

            FPCapsule hitting = new FPCapsule(V(-Quarter, 0, 0), V(Quarter, 0, 0), Frac(1, 5));
            Require(world.CheckCapsuleOverlap(hitting), "capsule must overlap the rotated box");

            FPCapsule missing = new FPCapsule(V(3, 3, 0), V(4, 3, 0), Frac(1, 5));
            Require(!world.CheckCapsuleOverlap(missing), "distant capsule must not overlap");
        }

        private static void TestOrientedCapsuleRest()
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld();
            AddGround(world, FP.One);
            int capsuleId = AddDynamicCapsule(world, V(0, 2, 0), Quarter, FP.One, FP.One);
            CYGPhysicsBody capsule = world.GetBody(capsuleId);
            capsule.Rotation = FPQuaternion.AngleAxis(FP.FromInt(90), FPVector3.Forward);

            for (int step = 0; step < 300; step++)
                world.Step();

            capsule = world.GetBody(capsuleId);
            RequireNear(capsule.Position.y, Quarter, Frac(1, 20),
                "horizontal capsule must rest on the ground at its radius");
            Require(capsule.Position.y > FP.Zero, "capsule must not tunnel through the ground");
        }

        private static void TestRollingFriction()
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld();
            AddGround(world, FP.One);
            int sphereId = AddDynamicSphere(world, V(0, Half, 0), Half, FP.One, FP.One, FP.Zero);
            world.GetBody(sphereId).LinearVelocity = V(5, 0, 0);

            for (int step = 0; step < 30; step++)
                world.Step();

            CYGPhysicsBody sphere = world.GetBody(sphereId);
            Require(sphere.AngularVelocity.z < -Frac(1, 10), "sliding sphere must gain rolling angular velocity");
            Require(sphere.LinearVelocity.x < FP.FromInt(5), "friction must slow the sphere");
        }

        #endregion

        #region Queries and triggers

        private static void TestQueries()
        {
            CYGPhysicsWorld world = NewWorld(false);

            int layerTwoBox = AddStaticBox(world, V(2, 0, 0), V(1, 1, 1), FP.Zero);
            world.GetBody(layerTwoBox).Layer = 2;

            int layerZeroSphere = AddStaticSphere(world, V(-2, 0, 0), Half);
            world.GetBody(layerZeroSphere).Layer = 0;

            int farBox = AddStaticBox(world, V(5, 0, 0), V(Half, Half, Half), FP.Zero);
            world.GetBody(farBox).Layer = 3;

            int upperBox = AddStaticBox(world, V(0, 2, 0), V(1, 1, 1), FP.Zero);
            world.GetBody(upperBox).Layer = 0;

            FPRay forward = new FPRay(FPVector3.Zero, V(1, 0, 0));
            CYGPhysicsHit hit = world.Raycast(forward, FP.FromInt(10));
            Require(hit.Hit, "forward ray must hit the layer-two box");
            Require(hit.BodyId == layerTwoBox, "forward ray must report the nearest box");
            RequireNear(hit.Distance, FP.One, Frac(1, 1000), "forward ray distance");

            Require(!world.Raycast(forward, FP.FromInt(10), 1u << 0).Hit,
                "layer mask must reject the layer-two box and the far layer-three box");
            Require(world.Raycast(forward, FP.FromInt(10), 1u << 2).Hit,
                "layer mask must accept the layer-two box");

            FPRay backward = new FPRay(FPVector3.Zero, V(-1, 0, 0));
            hit = world.Raycast(backward, FP.FromInt(10), 1u << 0);
            Require(hit.Hit && hit.BodyId == layerZeroSphere, "backward ray must hit the layer-zero sphere");
            RequireNear(hit.Distance, Frac(3, 2), Frac(1, 1000), "backward ray distance");

            List<CYGPhysicsHit> allHits = new List<CYGPhysicsHit>();
            int hitCount = world.RaycastAll(forward, FP.FromInt(10), allHits);
            Require(hitCount == 2, "RaycastAll must return both boxes along the ray");
            Require(allHits[0].BodyId == layerTwoBox && allHits[1].BodyId == farBox,
                "RaycastAll must be sorted by distance then body id");
            RequireNear(allHits[0].Distance, FP.One, Frac(1, 1000), "RaycastAll first distance");
            RequireNear(allHits[1].Distance, Frac(9, 2), Frac(1, 1000), "RaycastAll second distance");

            FPSphere querySphere = new FPSphere(FPVector3.Zero, Quarter);
            hit = world.SphereCast(querySphere, V(1, 0, 0), FP.FromInt(10));
            Require(hit.Hit && hit.BodyId == layerTwoBox, "sphere cast must hit the near box");
            RequireNear(hit.Distance, Frac(3, 4), Frac(1, 100), "sphere cast distance");

            List<CYGOverlapResult> overlaps = new List<CYGOverlapResult>();
            int overlapCount = world.OverlapSphere(new FPSphere(FPVector3.Zero, FP.FromInt(3)), overlaps);
            Require(overlapCount == 3, "overlap sphere count (got " + overlapCount + ")");
            Require(overlaps[0].BodyId == layerTwoBox, "overlap results must be sorted by distance then id (first)");
            Require(overlaps[1].BodyId == upperBox, "overlap equal-distance tie must use ascending body id");
            Require(overlaps[2].BodyId == layerZeroSphere, "overlap results must be sorted by distance then id (last)");
            RequireNear(overlaps[0].Distance, FP.One, Frac(1, 1000), "overlap box surface distance");
            RequireNear(overlaps[2].Distance, Frac(3, 2), Frac(1, 1000), "overlap sphere surface distance");

            overlapCount = world.OverlapSphere(new FPSphere(FPVector3.Zero, FP.FromInt(3)), overlaps, 1u << 2);
            Require(overlapCount == 1 && overlaps[0].BodyId == layerTwoBox, "overlap sphere layer filter");

            FPVector3 queryHalf = V(Frac(7, 5), Frac(7, 5), Frac(7, 5));
            FPAABB queryAabb = new FPAABB(-queryHalf, queryHalf);
            overlapCount = world.OverlapAABB(queryAabb, overlaps);
            Require(overlapCount == 2, "overlap AABB count (got " + overlapCount + ")");
            Require(overlaps[0].BodyId == layerTwoBox && overlaps[1].BodyId == upperBox,
                "overlap AABB must be sorted by body id for equal distances");
        }

        private static void TestTriggerEvents()
        {
            int enters;
            int exits;
            int stays;
            int collisions;
            int enterBodyA;
            int enterBodyB;
            List<int[]> firstSequence = RunTriggerScenario(
                out enters, out exits, out stays, out collisions, out enterBodyA, out enterBodyB);

            Require(enters == 1, "expected exactly one trigger enter (got " + enters + ")");
            Require(exits == 1, "expected exactly one trigger exit (got " + exits + ")");
            Require(stays > 0, "expected trigger stay events");
            Require(collisions == 0, "trigger overlap must not emit collision events");
            Require(firstSequence.Count > 2, "expected a full enter/stay/exit sequence");
            Require(firstSequence[0][0] == (int)CYGPhysicsEventType.TriggerEnter,
                "first trigger event must be enter");
            Require(firstSequence[firstSequence.Count - 1][0] == (int)CYGPhysicsEventType.TriggerExit,
                "last trigger event must be exit");
            Require(enterBodyA == 0 && enterBodyB == 1,
                "trigger events must identify the trigger as body A");

            int secondEnters;
            int secondExits;
            int secondStays;
            int secondCollisions;
            int secondBodyA;
            int secondBodyB;
            List<int[]> secondSequence = RunTriggerScenario(
                out secondEnters, out secondExits, out secondStays, out secondCollisions,
                out secondBodyA, out secondBodyB);

            Require(SameSequence(firstSequence, secondSequence),
                "trigger event sequence must be identical across two identical runs");
            Require(secondEnters == enters && secondExits == exits && secondStays == stays &&
                    secondCollisions == collisions,
                "trigger event counts must be deterministic");
        }

        private static List<int[]> RunTriggerScenario(
            out int enters,
            out int exits,
            out int stays,
            out int collisions,
            out int enterBodyA,
            out int enterBodyB)
        {
            CYGPhysicsWorld world = NewWorld(false);

            CYGPhysicsBody trigger = CYGPhysicsBody.CreateStatic(CYGCollider.Box(V(1, 1, 1)));
            trigger.Position = FPVector3.Zero;
            trigger.Collider.IsTrigger = true;
            int triggerId = world.AddBody(trigger);

            int sphereId = AddDynamicSphere(world, V(-4, 0, 0), Half, 1, Frac(1, 2), FP.Zero);
            world.GetBody(sphereId).LinearVelocity = V(2, 0, 0);

            enters = 0;
            exits = 0;
            stays = 0;
            collisions = 0;
            enterBodyA = -1;
            enterBodyB = -1;

            List<int[]> sequence = new List<int[]>();
            for (int step = 0; step < 180; step++)
            {
                world.Step();
                for (int i = 0; i < world.Events.Count; i++)
                {
                    CYGPhysicsEvent evt = world.Events[i];
                    sequence.Add(new[] { (int)evt.Type, evt.BodyA, evt.BodyB });

                    switch (evt.Type)
                    {
                        case CYGPhysicsEventType.TriggerEnter:
                            enters++;
                            enterBodyA = evt.BodyA;
                            enterBodyB = evt.BodyB;
                            break;
                        case CYGPhysicsEventType.TriggerStay:
                            stays++;
                            break;
                        case CYGPhysicsEventType.TriggerExit:
                            exits++;
                            break;
                        default:
                            collisions++;
                            break;
                    }
                }
            }

            Require(world.GetBody(sphereId).Position.x > Frac(3, 2),
                "sphere must have passed through the trigger volume");
            Require(world.GetBody(triggerId) != null, "trigger body must remain active");
            return sequence;
        }

        private static bool SameSequence(List<int[]> first, List<int[]> second)
        {
            if (first.Count != second.Count)
                return false;
            for (int i = 0; i < first.Count; i++)
            {
                if (first[i].Length != second[i].Length)
                    return false;
                for (int j = 0; j < first[i].Length; j++)
                {
                    if (first[i][j] != second[i][j])
                        return false;
                }
            }
            return true;
        }

        #endregion

        #region Narrowphase coverage and joints

        private static void TestNarrowphasePairCoverage()
        {
            CYGPhysicsWorld world = NewWorld(false);

            // Box versus box.
            AddStaticBox(world, V(-30, 0, 0), V(Half, Half, Half), Frac(1, 2));
            AddDynamicBox(world, V(Frac(-297, 10), 0, 0), V(Half, Half, Half), 1, Frac(1, 2), FP.Zero);

            // Sphere versus sphere.
            AddStaticSphere(world, V(-18, 0, 0), Half);
            AddDynamicSphere(world, V(Frac(-177, 10), 0, 0), Half, 1, Frac(1, 2), FP.Zero);

            // Sphere versus box.
            AddStaticBox(world, V(-6, 0, 0), V(Half, Half, Half), Frac(1, 2));
            AddDynamicSphere(world, V(Frac(-57, 10), 0, 0), Half, 1, Frac(1, 2), FP.Zero);

            // Capsule versus box.
            AddStaticBox(world, V(6, 0, 0), V(Half, Half, Half), Frac(1, 2));
            AddDynamicCapsule(world, V(Frac(63, 10), 0, 0), Quarter, FP.FromInt(2), 1);

            // Capsule versus sphere.
            CYGPhysicsBody staticCapsule = CYGPhysicsBody.CreateStatic(CYGCollider.Capsule(Quarter, FP.FromInt(2)));
            staticCapsule.Position = V(18, 0, 0);
            staticCapsule.Friction = Frac(1, 2);
            world.AddBody(staticCapsule);
            AddDynamicSphere(world, V(Frac(184, 10), 0, 0), Half, 1, Frac(1, 2), FP.Zero);

            // Capsule versus capsule.
            CYGPhysicsBody staticCapsule2 = CYGPhysicsBody.CreateStatic(CYGCollider.Capsule(Quarter, FP.FromInt(2)));
            staticCapsule2.Position = V(30, 0, 0);
            staticCapsule2.Friction = Frac(1, 2);
            world.AddBody(staticCapsule2);
            AddDynamicCapsule(world, V(Frac(304, 10), 0, 0), Quarter, FP.FromInt(2), 1);

            world.Step();

            int collisionEnters = 0;
            for (int i = 0; i < world.Events.Count; i++)
            {
                CYGPhysicsEvent evt = world.Events[i];
                if (evt.Type == CYGPhysicsEventType.CollisionEnter)
                {
                    collisionEnters++;
                    Require(evt.BodyA < evt.BodyB, "collision events must order bodies by ascending id");
                }
            }

            Require(collisionEnters == 6,
                "all six shape pairs must generate a contact (got " + collisionEnters + ")");
        }

        private static void TestDistanceJoint()
        {
            CYGPhysicsWorld world = NewWorld(false);
            int anchorId = AddStaticSphere(world, FPVector3.Zero, Quarter);
            int bobId = AddDynamicSphere(world, V(6, 0, 0), Half, 1, Frac(1, 2), FP.Zero);
            world.AddJoint(CYGJoint.CreateDistance(anchorId, bobId, FPVector3.Zero, FPVector3.Zero, FP.FromInt(5)));

            FP maxError = FP.Zero;
            for (int step = 0; step < 300; step++)
            {
                world.Step();
                FP distance = FPVector3.Distance(world.GetBody(anchorId).Position, world.GetBody(bobId).Position);
                FP error = FP.Abs(distance - FP.FromInt(5));
                if (error > maxError)
                    maxError = error;
            }

            FP finalDistance = FPVector3.Distance(
                world.GetBody(anchorId).Position,
                world.GetBody(bobId).Position);
            RequireNear(finalDistance, FP.FromInt(5), Frac(1, 50), "distance joint must converge to its rest length");
            Require(maxError <= Frac(1, 50), "distance joint must stay near its rest length");
            Require(world.GetBody(bobId).LinearVelocity.Magnitude < Frac(1, 100),
                "distance joint must remove relative velocity");
        }

        private static void TestSpringJoint()
        {
            CYGPhysicsWorld world = NewWorld(false);
            int anchorId = AddStaticSphere(world, FPVector3.Zero, Quarter);
            int bobId = AddDynamicSphere(world, V(0, -3, 0), Half, 1, Frac(1, 2), FP.Zero);
            world.AddJoint(CYGJoint.CreateSpring(anchorId, bobId, FPVector3.Zero, FPVector3.Zero,
                FP.FromInt(2), FP.FromInt(20), FP.FromInt(2)));

            FP initialDistance = FPVector3.Distance(
                world.GetBody(anchorId).Position,
                world.GetBody(bobId).Position);

            for (int step = 0; step < 30; step++)
                world.Step();

            CYGPhysicsBody bob = world.GetBody(bobId);
            FP distance = FPVector3.Distance(world.GetBody(anchorId).Position, bob.Position);
            Require(initialDistance == FP.FromInt(3), "spring test must start at distance 3");
            Require(distance < Frac(29, 10), "spring must pull the bob toward its rest length");
            Require(bob.Position.y > -3, "spring must lift the bob");
            Require(distance != initialDistance, "spring must produce motion");
        }

        private static void TestJointTorque()
        {
            CYGPhysicsWorld world = NewWorld(false);
            int anchorId = AddStaticSphere(world, FPVector3.Zero, Quarter);
            int boxId = AddDynamicBox(world, V(0, 0, 4), V(Half, Half, Half), FP.One,
                Frac(1, 2), FP.Zero);
            world.AddJoint(CYGJoint.CreateSpring(anchorId, boxId,
                V(0, 0, 1), V(Half, 0, 0), FP.FromInt(2), FP.FromInt(20), FP.FromInt(2)));

            for (int step = 0; step < 30; step++)
                world.Step();

            CYGPhysicsBody box = world.GetBody(boxId);
            Require(box.AngularVelocity.SqrMagnitude > Frac(1, 100),
                "off-centre distance-joint anchors must generate angular velocity");
        }

        #endregion

        #region CCD and collision-world adapter

        private static void TestCcdPreventsTunneling()
        {
            CYGPhysicsWorld world = NewWorld(false);
            AddStaticBox(world, FPVector3.Zero, V(Frac(1, 20), 5, 5), Frac(1, 2));
            int sphereId = AddDynamicSphere(world, V(-5, 0, 0), Frac(1, 10), 1, FP.Zero, FP.Zero);
            CYGPhysicsBody sphere = world.GetBody(sphereId);
            sphere.EnableCcd = true;
            sphere.LinearVelocity = V(200, 0, 0);

            for (int step = 0; step < 10; step++)
            {
                world.Step();
                Require(sphere.Position.x <= -Frac(1, 20) + Frac(1, 1000),
                    "CCD sphere must not cross the static wall (x=" + sphere.Position.x + ")");
            }

            RequireNear(sphere.Position.x, -Frac(3, 20), Frac(1, 100), "CCD must clamp the sphere to the wall surface");
            Require(FP.Abs(sphere.LinearVelocity.x) < Frac(1, 100),
                "CCD contact must remove the approaching velocity");

            CYGPhysicsWorld tunnelWorld = NewWorld(false);
            AddStaticBox(tunnelWorld, FPVector3.Zero, V(Frac(1, 20), 5, 5), Frac(1, 2));
            int tunnelSphereId = AddDynamicSphere(tunnelWorld, V(-5, 0, 0), Frac(1, 10), 1, FP.Zero, FP.Zero);
            tunnelWorld.GetBody(tunnelSphereId).LinearVelocity = V(200, 0, 0);
            for (int step = 0; step < 10; step++)
                tunnelWorld.Step();
            Require(tunnelWorld.GetBody(tunnelSphereId).Position.x > FP.FromInt(10),
                "without CCD the fast sphere must tunnel through the thin wall as documented");
        }

        private static void TestCollisionWorldInterface()
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld();
            AddGround(world, Frac(4, 5));

            CYGPhysicsBody trigger = CYGPhysicsBody.CreateStatic(CYGCollider.Box(V(1, 1, 1)));
            trigger.Position = V(0, 4, 0);
            trigger.Collider.IsTrigger = true;
            int triggerId = world.AddBody(trigger);

            ICollisionWorld collisionWorld = world;

            FPRay down = new FPRay(V(0, 6, 0), V(0, -1, 0));
            CYGPhysicsHit publicHit = world.Raycast(down, FP.FromInt(10));
            Require(publicHit.Hit && publicHit.BodyId == triggerId,
                "public raycast must include trigger colliders by default");

            FPRaycastHit interfaceHit;
            Require(collisionWorld.Raycast(down, FP.FromInt(10), out interfaceHit),
                "ICollisionWorld raycast must hit the ground");
            RequireNear(interfaceHit.Distance, FP.FromInt(6), Frac(1, 100),
                "ICollisionWorld raycast must ignore triggers");

            FPSphere probeSphere = new FPSphere(V(0, 2, 0), Half);
            Require(collisionWorld.SphereCast(probeSphere, V(0, -1, 0), FP.FromInt(10), out interfaceHit),
                "ICollisionWorld sphere cast must hit the ground");
            RequireNear(interfaceHit.Distance, Frac(3, 2), Frac(1, 100),
                "ICollisionWorld sphere cast distance");

            FPCapsule probeCapsule = FPCapsule.CreateVertical(V(0, 2, 0), FP.FromInt(2), Quarter);
            Require(collisionWorld.CapsuleCast(probeCapsule, V(0, -1, 0), FP.FromInt(10), out interfaceHit),
                "ICollisionWorld capsule cast must hit the ground");
            RequireNear(interfaceHit.Distance, FP.One, Frac(1, 100),
                "ICollisionWorld capsule cast distance");

            FPCapsule embedded = FPCapsule.CreateVertical(V(0, Quarter, 0), FP.FromInt(2), Quarter);
            Require(collisionWorld.CheckCapsuleOverlap(embedded),
                "embedded character capsule must report an overlap");
            FPCapsule far = FPCapsule.CreateVertical(V(0, 10, 0), FP.FromInt(2), Quarter);
            Require(!collisionWorld.CheckCapsuleOverlap(far),
                "far character capsule must not report an overlap");
        }

        #endregion

        #region Determinism, snapshots and allocation

        private static CYGPhysicsWorld BuildDeterminismScene()
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld();
            AddGround(world, Frac(4, 5));

            AddDynamicBox(world, V(0, Half, 0), V(Half, Half, Half), 1, Frac(3, 5), FP.Zero);
            AddDynamicBox(world, V(Frac(1, 10), Frac(3, 2), 0), V(Half, Half, Half), 1, Frac(3, 5), FP.Zero);
            AddDynamicBox(world, V(Frac(-1, 10), Frac(5, 2), 0), V(Half, Half, Half), 1, Frac(3, 5), FP.Zero);

            AddDynamicSphere(world, V(3, 6, 0), Half, Frac(3, 4), Frac(1, 2), FP.Zero);

            int slider = AddDynamicBox(world, V(-2, Half, 0), V(Half, Half, Half), 1, Frac(3, 10), FP.Zero);
            world.GetBody(slider).LinearVelocity = V(3, 0, 0);

            int anchor = AddStaticSphere(world, V(-5, 6, 0), Quarter);
            int bob = AddDynamicSphere(world, V(-3, 6, 0), Quarter, Half, Frac(1, 2), FP.Zero);
            world.AddJoint(CYGJoint.CreateDistance(anchor, bob, FPVector3.Zero, FPVector3.Zero, FP.FromInt(2)));

            CYGPhysicsBody trigger = CYGPhysicsBody.CreateStatic(CYGCollider.Box(V(1, 1, 1)));
            trigger.Position = V(3, Frac(5, 2), 0);
            trigger.Collider.IsTrigger = true;
            world.AddBody(trigger);

            return world;
        }

        private static void TestDeterminismAndSnapshot()
        {
            CYGPhysicsWorld first = BuildDeterminismScene();
            CYGPhysicsWorld second = BuildDeterminismScene();

            Require(first.ActiveBodyCount == 9, "determinism scene body count");
            Require(first.ComputeChecksum() == second.ComputeChecksum(),
                "identical fully built worlds must start with equal checksums");

            for (int step = 1; step <= 300; step++)
            {
                first.Step();
                second.Step();
                if (step % 100 == 0)
                {
                    Require(first.ComputeChecksum() == second.ComputeChecksum(),
                        "identical worlds diverged at tick " + step);
                }
            }

            CYGPhysicsSnapshot snapshot = first.CaptureSnapshot();
            uint snapshotChecksum = snapshot.ComputeChecksum();
            Require(snapshotChecksum == first.ComputeChecksum(),
                "snapshot checksum must equal the world checksum");
            Require(snapshot.Tick == 300, "snapshot tick must match");
            Require(snapshot.BodyCount == first.ActiveBodyCount, "snapshot body count must match");
            Require(snapshot.ContactPairCount > 0, "snapshot should capture resting contact pairs");

            for (int step = 0; step < 100; step++)
            {
                first.Step();
                second.Step();
            }
            Require(first.ComputeChecksum() == second.ComputeChecksum(),
                "identical worlds must match at tick 400");
            uint checksumAt400 = first.ComputeChecksum();

            first.RestoreSnapshot(snapshot);
            Require(first.Tick == 300, "restore must rewind the tick");
            Require(first.ComputeChecksum() == snapshotChecksum,
                "restore must reproduce the snapshot checksum");

            for (int step = 0; step < 100; step++)
                first.Step();
            Require(first.ComputeChecksum() == checksumAt400,
                "restored world must continue bit-identically to tick 400");

            for (int step = 0; step < 200; step++)
            {
                first.Step();
                second.Step();
            }
            Require(first.ComputeChecksum() == second.ComputeChecksum(),
                "identical worlds must match after 600 steps");

            CYGPhysicsSnapshot finalSnapshot = first.CaptureSnapshot();
            Require(finalSnapshot.ComputeChecksum() == first.ComputeChecksum(),
                "final snapshot checksum must equal the final world checksum");

            CYGPhysicsWorld wrongWorld = new CYGPhysicsWorld();
            AddGround(wrongWorld, Frac(4, 5));
            RequireThrows<InvalidOperationException>(
                () => wrongWorld.RestoreSnapshot(snapshot),
                "restoring a snapshot into a world with a different body set must throw");
        }

        private static void TestFixedStepAllocation()
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld();
            AddGround(world, Frac(4, 5));
            int sphereId = AddDynamicSphere(world, V(0, 3, 0), Half, 1, Frac(1, 2), FP.Zero);
            int anchorId = AddStaticSphere(world, V(0, 6, 0), Quarter);
            world.AddJoint(CYGJoint.CreateDistance(anchorId, sphereId, FPVector3.Zero, FPVector3.Zero,
                FP.FromInt(3)));

            for (int step = 0; step < 180; step++)
                world.Step();

            long before = GC.GetAllocatedBytesForCurrentThread();
            for (int step = 0; step < 120; step++)
                world.Step();
            long allocated = GC.GetAllocatedBytesForCurrentThread() - before;

            Require(allocated == 0, "fixed Step must not allocate after warmup (allocated " + allocated + " bytes)");
        }

        #endregion
    }
}
