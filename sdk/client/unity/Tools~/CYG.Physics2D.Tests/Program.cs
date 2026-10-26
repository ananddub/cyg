//-----------------------------------------------------------------------------
// Program.cs
// Acceptance tests for the deterministic fixed-point 2D physics MVP.
//-----------------------------------------------------------------------------

using System;
using System.Collections.Generic;

namespace CookYourGame.Sim.Tests
{
    internal static class Program
    {
        private static int _assertions;

        private static int Main()
        {
            try
            {
                TestCircleRest();
                TestRestitution();
                TestCircleBoxBounce();
                TestSlidingFriction();
                TestRotatedBoxSeparation();
                TestQueries();
                TestTriggers();
                TestDistanceJoint();
                TestSpringJoint();
                TestStackedBoxes();
                TestLargeScaleDeterminism();
                TestDeterminismAndSnapshot();
                TestNoStepAllocations();

                Console.WriteLine("CYG physics2d tests passed: assertions=" + _assertions);
                return 0;
            }
            catch (Exception exception)
            {
                Console.Error.WriteLine("CYG physics2d tests failed: " + exception);
                return 1;
            }
        }

        //---------------------------------------------------------------------
        // Falling / resting
        //---------------------------------------------------------------------
        private static void TestCircleRest()
        {
            CYGPhysics2DWorld world = new CYGPhysics2DWorld(new FPVector2(0, -10));
            world.AddBody(CYGPhysics2DBody.Box(
                new FPVector2(0, 0),
                new FPVector2(5, F(1, 2)),
                CYGPhysics2DBodyType.Static,
                FP.One));
            int circleId = world.AddBody(CYGPhysics2DBody.Circle(new FPVector2(0, 4), F(1, 2)));

            FP minY = FP.MaxValue;
            FP maxY = FP.MinValue;
            FP minX = FP.MaxValue;
            FP maxX = FP.MinValue;

            for (int step = 0; step < 300; step++)
            {
                world.Step();
                if (step >= 270)
                {
                    CYGPhysics2DBody body;
                    if (!world.TryGetBody(circleId, out body))
                    {
                        throw new InvalidOperationException("circle disappeared");
                    }

                    minY = FP.Min(minY, body.Position.y);
                    maxY = FP.Max(maxY, body.Position.y);
                    minX = FP.Min(minX, body.Position.x);
                    maxX = FP.Max(maxX, body.Position.x);
                }
            }

            CYGPhysics2DBody resting;
            Require(world.TryGetBody(circleId, out resting), "circle should still exist");
            Near(resting.Position.y, FP.One, F(1, 50), "circle rest height");
            Require(maxY - minY <= F(1, 1000), "circle vertical jitter in last 30 steps");
            Require(maxX - minX <= F(1, 1000), "circle horizontal jitter in last 30 steps");
            Require(resting.Velocity.SqrMagnitude <= F(1, 400), "circle should be at rest");
            Require(!resting.IsAwake, "circle should sleep after resting");

            // Same scene with sleeping disabled: the solver itself must not jitter.
            CYGPhysics2DWorld awakeWorld = new CYGPhysics2DWorld(new FPVector2(0, -10));
            awakeWorld.EnableSleeping = false;
            awakeWorld.AddBody(CYGPhysics2DBody.Box(
                new FPVector2(0, 0),
                new FPVector2(5, F(1, 2)),
                CYGPhysics2DBodyType.Static,
                FP.One));
            int awakeCircle = awakeWorld.AddBody(CYGPhysics2DBody.Circle(new FPVector2(0, 4), F(1, 2)));

            FP awakeMinY = FP.MaxValue;
            FP awakeMaxY = FP.MinValue;
            for (int step = 0; step < 300; step++)
            {
                awakeWorld.Step();
                if (step >= 270)
                {
                    CYGPhysics2DBody body;
                    awakeWorld.TryGetBody(awakeCircle, out body);
                    awakeMinY = FP.Min(awakeMinY, body.Position.y);
                    awakeMaxY = FP.Max(awakeMaxY, body.Position.y);
                }
            }

            CYGPhysics2DBody awakeBody;
            awakeWorld.TryGetBody(awakeCircle, out awakeBody);
            Require(awakeBody.IsAwake, "sleeping was disabled");
            Near(awakeBody.Position.y, FP.One, F(1, 50), "awake circle rest height");
            Require(awakeMaxY - awakeMinY <= F(1, 200), "awake circle must not jitter");
            Require(awakeBody.Velocity.SqrMagnitude <= F(1, 400), "awake circle speed must be near zero");
        }

        //---------------------------------------------------------------------
        // Restitution
        //---------------------------------------------------------------------
        private static void TestRestitution()
        {
            FP inelasticA;
            FP inelasticB;
            RunHeadOn(FP.Zero, out inelasticA, out inelasticB);
            Require(FP.Abs(inelasticA) <= F(1, 20), "inelastic body A should stop");
            Require(FP.Abs(inelasticB) <= F(1, 20), "inelastic body B should stop");

            FP elasticA;
            FP elasticB;
            RunHeadOn(FP.One, out elasticA, out elasticB);
            Near(elasticA, -FP.One, F(1, 20), "elastic body A velocity");
            Near(elasticB, FP.One, F(1, 20), "elastic body B velocity");
        }

        private static void RunHeadOn(FP restitution, out FP velocityA, out FP velocityB)
        {
            CYGPhysics2DWorld world = new CYGPhysics2DWorld(FPVector2.Zero);
            int bodyA = world.AddBody(CYGPhysics2DBody.Circle(new FPVector2(-2, 0), F(1, 2)));
            int bodyB = world.AddBody(CYGPhysics2DBody.Circle(new FPVector2(2, 0), F(1, 2)));
            world.SetMaterial(bodyA, FP.Zero, restitution);
            world.SetMaterial(bodyB, FP.Zero, restitution);
            world.SetVelocity(bodyA, new FPVector2(1, 0));
            world.SetVelocity(bodyB, new FPVector2(-1, 0));

            for (int step = 0; step < 200; step++)
            {
                world.Step();
            }

            CYGPhysics2DBody a;
            CYGPhysics2DBody b;
            world.TryGetBody(bodyA, out a);
            world.TryGetBody(bodyB, out b);
            velocityA = a.Velocity.x;
            velocityB = b.Velocity.x;
        }

        //---------------------------------------------------------------------
        // Circle versus static box bounce
        //---------------------------------------------------------------------
        private static void TestCircleBoxBounce()
        {
            CYGPhysics2DWorld world = new CYGPhysics2DWorld(new FPVector2(0, -10));
            int floor = AddStaticBox(world, new FPVector2(0, 0), new FPVector2(6, F(1, 2)));
            world.SetMaterial(floor, FP.Zero, FP.One);

            int circle = world.AddBody(CYGPhysics2DBody.Circle(new FPVector2(0, 3), F(1, 2)));
            world.SetMaterial(circle, FP.Zero, FP.One);

            bool touched = false;
            bool rose = false;
            FP highestAfterBounce = FP.Zero;

            for (int step = 0; step < 300; step++)
            {
                world.Step();

                CYGPhysics2DBody body;
                Require(world.TryGetBody(circle, out body), "bouncing circle should exist");

                if (body.Position.y <= FP.One + F(1, 20))
                {
                    touched = true;
                }

                if (body.Velocity.y > FP.Zero && touched)
                {
                    rose = true;
                }

                if (rose)
                {
                    highestAfterBounce = FP.Max(highestAfterBounce, body.Position.y);
                }

                Require(body.Position.y > F(9, 10), "bouncing circle must not tunnel through the box");
            }

            Require(touched, "bouncing circle should reach the box");
            Require(rose, "circle should bounce upward off the box");
            Require(highestAfterBounce >= F(19, 10), "circle-box bounce should return most of its height");
        }

        //---------------------------------------------------------------------
        // Friction
        //---------------------------------------------------------------------
        private static void TestSlidingFriction()
        {
            FP frictionlessSpeed;
            FP frictionlessDistance;
            RunSlide(FP.Zero, out frictionlessSpeed, out frictionlessDistance);

            FP frictionSpeed;
            FP frictionDistance;
            RunSlide(F(1, 2), out frictionSpeed, out frictionDistance);

            Require(frictionlessSpeed >= F(49, 10), "frictionless box should keep sliding");
            Require(frictionSpeed <= F(1, 2), "friction box should slow down");
            Require(frictionlessSpeed - frictionSpeed >= F(3, 1), "friction must change speed");
            Require(frictionlessDistance - frictionDistance >= F(3, 1), "friction must shorten travel");
        }

        private static void RunSlide(FP friction, out FP speed, out FP distance)
        {
            CYGPhysics2DWorld world = new CYGPhysics2DWorld(new FPVector2(0, -10));
            int floor = world.AddBody(CYGPhysics2DBody.Box(
                new FPVector2(0, 0),
                new FPVector2(30, F(1, 2)),
                CYGPhysics2DBodyType.Static,
                FP.One));
            int box = world.AddBody(CYGPhysics2DBody.Box(
                new FPVector2(0, F(3, 4)),
                new FPVector2(F(3, 4), F(1, 4))));
            world.SetMaterial(floor, friction, FP.Zero);
            world.SetMaterial(box, friction, FP.Zero);
            world.SetVelocity(box, new FPVector2(5, 0));

            for (int step = 0; step < 120; step++)
            {
                world.Step();
            }

            CYGPhysics2DBody body;
            world.TryGetBody(box, out body);
            speed = FP.Abs(body.Velocity.x);
            distance = body.Position.x;
        }

        //---------------------------------------------------------------------
        // Rotated box versus box SAT / positional correction
        //---------------------------------------------------------------------
        private static void TestRotatedBoxSeparation()
        {
            CYGPhysics2DWorld world = new CYGPhysics2DWorld(FPVector2.Zero);
            int staticId = world.AddBody(CYGPhysics2DBody.Box(
                new FPVector2(0, 0),
                new FPVector2(F(1, 2), F(1, 2)),
                FP.Pi / 6,
                CYGPhysics2DBodyType.Static,
                FP.One));
            int dynamicId = world.AddBody(CYGPhysics2DBody.Box(
                new FPVector2(0, 1),
                new FPVector2(F(1, 2), F(1, 2)),
                FP.Zero,
                CYGPhysics2DBodyType.Dynamic,
                FP.One));

            CYGPhysics2DBody staticBox;
            CYGPhysics2DBody dynamicBox;
            world.TryGetBody(staticId, out staticBox);
            world.TryGetBody(dynamicId, out dynamicBox);

            FP before = BoxPenetration(staticBox, dynamicBox);
            Require(before >= F(1, 10), "rotated boxes should start deeply overlapped");

            FPVector2 start = dynamicBox.Position;
            for (int step = 0; step < 120; step++)
            {
                world.Step();
            }

            world.TryGetBody(staticId, out staticBox);
            world.TryGetBody(dynamicId, out dynamicBox);

            FP after = BoxPenetration(staticBox, dynamicBox);
            Require(after <= F(1, 50), "rotated boxes should separate");
            Require(FPVector2.Distance(dynamicBox.Position, start) >= F(1, 10), "dynamic box should be pushed out");
            Near(dynamicBox.Angle, FP.Zero, F(1, 100), "translation-only correction must not spin the box");
        }

        // Test-side independent SAT overlap (minimum projection overlap).
        private static FP BoxPenetration(in CYGPhysics2DBody a, in CYGPhysics2DBody b)
        {
            FPVector2[] axes = new FPVector2[4];
            axes[0] = BoxAxis(a, 0);
            axes[1] = BoxAxis(a, 1);
            axes[2] = BoxAxis(b, 0);
            axes[3] = BoxAxis(b, 1);

            FP minimum = FP.MaxValue;
            for (int i = 0; i < axes.Length; i++)
            {
                FPVector2 axis = axes[i];
                FP overlap = RadiusAlong(a, axis) + RadiusAlong(b, axis)
                    - FP.Abs(FPVector2.Dot(b.Position - a.Position, axis));
                if (overlap <= FP.Zero)
                {
                    return overlap;
                }

                minimum = FP.Min(minimum, overlap);
            }

            return minimum;
        }

        private static FPVector2 BoxAxis(in CYGPhysics2DBody body, int index)
        {
            FP cos = FP.Cos(body.Angle);
            FP sin = FP.Sin(body.Angle);
            return index == 0
                ? new FPVector2(cos, sin)
                : new FPVector2(-sin, cos);
        }

        private static FP RadiusAlong(in CYGPhysics2DBody body, FPVector2 axis)
        {
            if (body.Shape == CYGPhysics2DShapeType.Circle)
            {
                return body.Radius;
            }

            FPVector2 right = BoxAxis(body, 0);
            FPVector2 up = BoxAxis(body, 1);
            return FP.Abs(FPVector2.Dot(axis, right)) * body.HalfExtents.x
                + FP.Abs(FPVector2.Dot(axis, up)) * body.HalfExtents.y;
        }

        //---------------------------------------------------------------------
        // Raycast and overlap queries
        //---------------------------------------------------------------------
        private static void TestQueries()
        {
            CYGPhysics2DWorld world = new CYGPhysics2DWorld(FPVector2.Zero);
            int nearLow = AddStaticCircle(world, new FPVector2(5, 0), F(1, 2));
            int nearHigh = AddStaticCircle(world, new FPVector2(5, 0), F(1, 2));
            int far = AddStaticCircle(world, new FPVector2(8, 0), F(1, 2));

            CYGPhysics2DRaycastHit hit;
            Require(
                world.Raycast(new FPVector2(0, 0), new FPVector2(2, 0), 20, out hit),
                "ray should hit nearest circle");
            Equal(nearLow, hit.BodyId, "equal distances should pick the lower body id");
            Near(hit.Distance, F(9, 2), F(1, 1000), "ray distance to circle");
            Near(hit.Point.x, F(9, 2), F(1, 1000), "ray hit point x");
            Near(hit.Point.y, FP.Zero, F(1, 1000), "ray hit point y");
            Near(hit.Normal.x, -FP.One, F(1, 1000), "ray hit normal x");

            Require(world.DestroyBody(nearLow), "should destroy the tie winner");
            Require(
                world.Raycast(new FPVector2(0, 0), new FPVector2(1, 0), 20, out hit),
                "ray should hit the remaining tied circle");
            Equal(nearHigh, hit.BodyId, "destroyed body must not win raycast");

            world.SetLayer(far, 1, -1);
            world.SetLayer(nearHigh, 0, -1);
            Require(
                world.Raycast(new FPVector2(0, 0), new FPVector2(1, 0), 20, 1 << 1, out hit),
                "ray should respect the layer mask");
            Equal(far, hit.BodyId, "layer-filtered ray should hit the far body");
            Near(hit.Distance, F(15, 2), F(1, 1000), "layer-filtered ray distance");

            Require(
                !world.Raycast(new FPVector2(0, 0), new FPVector2(1, 0), F(4, 1), out hit),
                "ray beyond max distance must miss");
            Require(
                !world.Raycast(new FPVector2(0, 0), FPVector2.Zero, 10, out hit),
                "zero ray direction must miss");
            Require(
                !world.Raycast(new FPVector2(0, 0), new FPVector2(1, 0), 20, 1 << 5, out hit),
                "empty layer mask must miss");

            int rotatedBox = world.AddBody(CYGPhysics2DBody.Box(
                new FPVector2(0, 4),
                new FPVector2(F(3, 4), F(1, 4)),
                FP.HalfPi,
                CYGPhysics2DBodyType.Static,
                FP.One));
            Require(
                world.Raycast(new FPVector2(0, 0), new FPVector2(0, 1), 10, out hit),
                "ray should hit the rotated box");
            Equal(rotatedBox, hit.BodyId, "rotated box ray body id");
            Near(hit.Distance, F(13, 4), F(1, 100), "rotated box ray distance");
            Near(hit.Normal.y, -FP.One, F(1, 100), "rotated box ray normal");

            CYGPhysics2DWorld queryWorld = new CYGPhysics2DWorld(FPVector2.Zero);
            int a = AddStaticCircle(queryWorld, new FPVector2(1, 0), F(1, 2));
            int b = AddStaticCircle(queryWorld, new FPVector2(3, 0), F(1, 2));
            int c = AddStaticCircle(queryWorld, new FPVector2(3, 0), F(1, 2));
            int d = AddStaticBox(queryWorld, new FPVector2(5, 0), new FPVector2(F(1, 2), F(1, 2)));

            int[] buffer = new int[8];
            int count = queryWorld.OverlapCircle(new FPVector2(0, 0), 10, -1, buffer);
            Equal(4, count, "overlap circle count");
            Equal(a, buffer[0], "overlap result 0 by distance");
            Equal(b, buffer[1], "overlap result 1 tie by id");
            Equal(c, buffer[2], "overlap result 2 tie by id");
            Equal(d, buffer[3], "overlap result 3 by distance");

            count = queryWorld.OverlapCircle(new FPVector2(0, 0), 2, -1, buffer);
            Equal(1, count, "small overlap circle count");
            Equal(a, buffer[0], "small overlap body");

            int[] tiny = new int[2];
            count = queryWorld.OverlapCircle(new FPVector2(0, 0), 10, -1, tiny);
            Equal(4, count, "overlap should return total count for small buffers");
            Equal(a, tiny[0], "small buffer first result");
            Equal(b, tiny[1], "small buffer second result");

            queryWorld.SetLayer(d, 1, -1);
            count = queryWorld.OverlapCircle(new FPVector2(0, 0), 10, 1, buffer);
            Equal(3, count, "layer 0 overlap count");
            count = queryWorld.OverlapCircle(new FPVector2(0, 0), 10, 1 << 1, buffer);
            Equal(1, count, "layer 1 overlap count");
            Equal(d, buffer[0], "layer 1 overlap body");

            count = queryWorld.OverlapAABB(new FPVector2(-1, -1), new FPVector2(6, 1), -1, buffer);
            Equal(4, count, "overlap AABB count");
            Equal(b, buffer[0], "overlap AABB result 0");
            Equal(c, buffer[1], "overlap AABB result 1");
            Equal(a, buffer[2], "overlap AABB result 2");
            Equal(d, buffer[3], "overlap AABB result 3");

            count = queryWorld.OverlapBox(new FPVector2(3, 0), new FPVector2(1, 1), -1, buffer);
            Equal(2, count, "overlap box count");
            Equal(b, buffer[0], "overlap box first");
            Equal(c, buffer[1], "overlap box second");

            Require(queryWorld.OverlapPoint(new FPVector2(1, 0), -1), "overlap point inside circle");
            Require(!queryWorld.OverlapPoint(new FPVector2(1, 1), -1), "overlap point outside circle");

            ICollisionWorld2D collisionWorld = queryWorld;
            int[] interfaceBuffer = new int[2];
            count = collisionWorld.OverlapAABB(new FPVector2(-1, -1), new FPVector2(2, 1), -1, interfaceBuffer);
            Equal(1, count, "interface overlap AABB count");
            Equal(a, interfaceBuffer[0], "interface overlap body");
            Require(
                collisionWorld.Raycast(new FPVector2(-5, 0), new FPVector2(1, 0), 20, out hit),
                "interface ray should hit");
            Equal(a, hit.BodyId, "interface ray body id");
        }

        //---------------------------------------------------------------------
        // Trigger callbacks
        //---------------------------------------------------------------------
        private static void TestTriggers()
        {
            CYGPhysics2DWorld world = new CYGPhysics2DWorld(FPVector2.Zero);
            CYGPhysics2DBody triggerBody = CYGPhysics2DBody.Circle(
                new FPVector2(0, 0),
                FP.One,
                CYGPhysics2DBodyType.Static,
                FP.One);
            triggerBody.IsTrigger = true;
            int trigger = world.AddBody(triggerBody);
            int mover = world.AddBody(CYGPhysics2DBody.Circle(new FPVector2(-4, 0), F(1, 2)));
            world.SetVelocity(mover, new FPVector2(2, 0));

            EventRecorder recorder = new EventRecorder();
            world.Callbacks = recorder;

            for (int step = 0; step < 220; step++)
            {
                world.Step();
            }

            Equal(1, recorder.TriggerEnters, "trigger enter count");
            Equal(1, recorder.TriggerExits, "trigger exit count");
            Require(recorder.TriggerStays >= 60, "trigger stay should fire while overlapping");
            Equal(0, recorder.CollisionEvents, "triggers must not raise collision events");
            Require(recorder.PairsAscending, "callback body ids must be ascending");
            Require(recorder.TriggerKinds.Count == recorder.TriggerEnters + recorder.TriggerStays + recorder.TriggerExits,
                "event kind accounting");
            Equal(0, recorder.TriggerKinds[0], "first trigger event is enter");
            Equal(2, recorder.TriggerKinds[recorder.TriggerKinds.Count - 1], "last trigger event is exit");
            for (int i = 1; i < recorder.TriggerKinds.Count - 1; i++)
            {
                if (recorder.TriggerKinds[i] != 1)
                {
                    throw new InvalidOperationException("trigger stays must be consecutive");
                }
            }

            CYGPhysics2DBody moverBody;
            world.TryGetBody(mover, out moverBody);
            Near(moverBody.Velocity.x, FP.Two, F(1, 100), "trigger must not change mover velocity");
            Near(moverBody.Position.x, F(10, 3), F(1, 50), "trigger must not deflect mover");
            Near(moverBody.Position.y, FP.Zero, F(1, 1000), "trigger must not deflect mover vertically");

            CYGPhysics2DBody triggerState;
            world.TryGetBody(trigger, out triggerState);
            Require(triggerState.Position == FPVector2.Zero, "static trigger must not move");
        }

        private sealed class EventRecorder : ICYGPhysics2DCallbacks
        {
            public readonly List<int> TriggerKinds = new List<int>();
            public int TriggerEnters;
            public int TriggerStays;
            public int TriggerExits;
            public int CollisionEvents;
            public bool PairsAscending = true;

            public void OnCollisionEnter(int bodyA, int bodyB)
            {
                CollisionEvents++;
                CheckPair(bodyA, bodyB);
            }

            public void OnCollisionStay(int bodyA, int bodyB)
            {
                CollisionEvents++;
                CheckPair(bodyA, bodyB);
            }

            public void OnCollisionExit(int bodyA, int bodyB)
            {
                CollisionEvents++;
                CheckPair(bodyA, bodyB);
            }

            public void OnTriggerEnter(int bodyA, int bodyB)
            {
                TriggerEnters++;
                TriggerKinds.Add(0);
                CheckPair(bodyA, bodyB);
            }

            public void OnTriggerStay(int bodyA, int bodyB)
            {
                TriggerStays++;
                TriggerKinds.Add(1);
                CheckPair(bodyA, bodyB);
            }

            public void OnTriggerExit(int bodyA, int bodyB)
            {
                TriggerExits++;
                TriggerKinds.Add(2);
                CheckPair(bodyA, bodyB);
            }

            private void CheckPair(int bodyA, int bodyB)
            {
                if (bodyA >= bodyB)
                {
                    PairsAscending = false;
                }
            }
        }

        //---------------------------------------------------------------------
        // Joints
        //---------------------------------------------------------------------
        private static void TestDistanceJoint()
        {
            CYGPhysics2DWorld world = new CYGPhysics2DWorld(FPVector2.Zero);
            int anchor = AddStaticCircle(world, FPVector2.Zero, F(1, 5));
            int bob = world.AddBody(CYGPhysics2DBody.Circle(new FPVector2(0, 2), F(1, 5)));
            int joint = world.CreateDistanceJoint(anchor, bob);
            Require(joint > 0, "distance joint id");
            Equal(1, world.JointCount, "distance joint count");

            CYGPhysics2DJoint jointState;
            Require(world.TryGetJoint(joint, out jointState), "distance joint should exist");
            Equal((int)CYGPhysics2DJointType.Distance, (int)jointState.Type, "joint type");
            Near(jointState.RestLength, FP.Two, F(1, 1000), "implicit distance joint rest length");

            FPVector2 start = new FPVector2(0, 2);
            world.SetVelocity(bob, new FPVector2(2, 0));

            FP maximumError = FP.Zero;
            for (int step = 0; step < 300; step++)
            {
                world.Step();
                CYGPhysics2DBody bobBody;
                world.TryGetBody(bob, out bobBody);
                FP error = FP.Abs(FPVector2.Distance(bobBody.Position, FPVector2.Zero) - jointState.RestLength);
                maximumError = FP.Max(maximumError, error);
            }

            Require(maximumError <= F(1, 50), "distance joint must hold its rest length");
            Require(world.DestroyJoint(joint), "distance joint should be destroyable");
            Equal(0, world.JointCount, "distance joint count after destroy");
        }

        private static void TestSpringJoint()
        {
            CYGPhysics2DWorld world = new CYGPhysics2DWorld(FPVector2.Zero);
            int anchor = AddStaticCircle(world, FPVector2.Zero, F(1, 5));
            int bob = world.AddBody(CYGPhysics2DBody.Circle(new FPVector2(0, 2), F(1, 5)));
            world.CreateSpringJoint(anchor, bob, FP.One, FP.FromInt(5), FP.One);

            FP distanceAt30 = FP.Zero;
            for (int step = 0; step < 300; step++)
            {
                world.Step();
                if (step == 29)
                {
                    CYGPhysics2DBody bobBody;
                    world.TryGetBody(bob, out bobBody);
                    distanceAt30 = bobBody.Position.Magnitude;
                }
            }

            CYGPhysics2DBody finalBob;
            world.TryGetBody(bob, out finalBob);
            FP finalDistance = finalBob.Position.Magnitude;
            Require(distanceAt30 < F(19, 10), "spring should pull the body toward the rest length");
            Require(FP.Abs(finalDistance - FP.One) <= F(1, 5), "damped spring should settle near rest length");
        }

        //---------------------------------------------------------------------
        // Stacking
        //---------------------------------------------------------------------
        private static void TestStackedBoxes()
        {
            CYGPhysics2DWorld world = new CYGPhysics2DWorld(new FPVector2(0, -10));
            world.AddBody(CYGPhysics2DBody.Box(
                new FPVector2(0, 0),
                new FPVector2(6, F(1, 2)),
                CYGPhysics2DBodyType.Static,
                FP.One));

            const int BoxCount = 3;
            int[] boxes = new int[BoxCount];
            FP[] minY = new FP[BoxCount];
            FP[] maxY = new FP[BoxCount];
            for (int i = 0; i < BoxCount; i++)
            {
                boxes[i] = world.AddBody(CYGPhysics2DBody.Box(
                    new FPVector2(0, 1 + i),
                    new FPVector2(F(1, 2), F(1, 2))));
                minY[i] = FP.MaxValue;
                maxY[i] = FP.MinValue;
            }

            for (int step = 0; step < 300; step++)
            {
                world.Step();
                if (step >= 240)
                {
                    for (int i = 0; i < BoxCount; i++)
                    {
                        CYGPhysics2DBody body;
                        world.TryGetBody(boxes[i], out body);
                        minY[i] = FP.Min(minY[i], body.Position.y);
                        maxY[i] = FP.Max(maxY[i], body.Position.y);
                    }
                }
            }

            for (int i = 0; i < BoxCount; i++)
            {
                CYGPhysics2DBody body;
                world.TryGetBody(boxes[i], out body);
                Near(body.Position.y, FP.FromInt(1 + i), F(1, 20), "stacked box height " + i);
                Near(body.Position.x, FP.Zero, F(1, 50), "stacked box horizontal drift " + i);
                Require(FP.Abs(body.Angle) <= F(1, 50), "stacked box rotation " + i);
                Require(maxY[i] - minY[i] <= F(1, 500), "stacked box jitter " + i);
            }
        }

        //---------------------------------------------------------------------
        // 300 dynamic bodies, 600 ticks, identical checksums
        //---------------------------------------------------------------------
        private static void TestLargeScaleDeterminism()
        {
            uint first = RunLargeScene();
            uint second = RunLargeScene();
            Equal(first, second, "two identical 300-body / 600-tick runs must match");
        }

        private static uint RunLargeScene()
        {
            CYGPhysics2DWorld world = new CYGPhysics2DWorld(new FPVector2(0, -10));
            world.AddBody(CYGPhysics2DBody.Box(
                new FPVector2(0, 0),
                new FPVector2(200, F(1, 2)),
                CYGPhysics2DBodyType.Static,
                FP.One));

            for (int index = 0; index < 300; index++)
            {
                int column = index % 20;
                int row = index / 20;
                FPVector2 position = new FPVector2(-95 + column * 10, 3 + row * 2);
                FP radius = F(2, 5);
                int id;
                if ((index & 1) == 0)
                {
                    id = world.AddBody(CYGPhysics2DBody.Box(
                        position,
                        new FPVector2(radius, radius),
                        F((index % 3) - 1, 20),
                        CYGPhysics2DBodyType.Dynamic,
                        FP.One + F(index % 3, 2)));
                }
                else
                {
                    id = world.AddBody(CYGPhysics2DBody.Circle(
                        position,
                        radius,
                        CYGPhysics2DBodyType.Dynamic,
                        FP.One + F(index % 3, 2)));
                }

                world.SetMaterial(id, F(1, 2), ((index % 7) == 0) ? F(1, 4) : FP.Zero);
            }

            Require(world.BodyCount == 301, "large scene should hold 300 dynamic bodies plus ground");
            for (int step = 0; step < 600; step++)
            {
                world.Step();
            }

            return world.ComputeChecksum();
        }

        //---------------------------------------------------------------------
        // Determinism, snapshot and restore
        //---------------------------------------------------------------------
        private static void TestDeterminismAndSnapshot()
        {
            CYGPhysics2DWorld first = BuildDeterminismScene();
            CYGPhysics2DWorld second = BuildDeterminismScene();
            Equal(first.BodyCount, second.BodyCount, "identical scene body counts");

            CYGPhysics2DWorldSnapshot snapshot = null;
            for (int step = 1; step <= 600; step++)
            {
                first.Step();
                second.Step();

                if ((step % 50) == 0)
                {
                    Equal(first.ComputeChecksum(), second.ComputeChecksum(), "checksum at step " + step);
                }

                if (step == 300)
                {
                    snapshot = second.CaptureSnapshot();
                    Equal(second.ComputeChecksum(), snapshot.ComputeChecksum(), "snapshot checksum");
                    Equal(300, snapshot.Tick, "snapshot tick");

                    int sleeping = 0;
                    for (int i = 0; i < snapshot.BodyCount; i++)
                    {
                        CYGPhysics2DBody body = snapshot.GetBody(i);
                        if (body.IsDynamic && !body.IsAwake)
                        {
                            sleeping++;
                        }
                    }

                    Require(sleeping > 0, "scene should contain sleeping bodies at tick 300");
                }
            }

            uint finalChecksum = first.ComputeChecksum();
            Equal(finalChecksum, second.ComputeChecksum(), "600-step deterministic checksum");

            second.RestoreSnapshot(snapshot);
            Equal(snapshot.ComputeChecksum(), second.ComputeChecksum(), "restore must reproduce the checksum exactly");

            for (int i = 0; i < snapshot.BodyCount; i++)
            {
                CYGPhysics2DBody expected = snapshot.GetBody(i);
                CYGPhysics2DBody actual;
                Require(second.TryGetBody(expected.Id, out actual), "restored body " + expected.Id + " exists");
                Require(actual.Position == expected.Position
                    && actual.Angle == expected.Angle
                    && actual.Velocity == expected.Velocity
                    && actual.AngularVelocity == expected.AngularVelocity
                    && actual.IsAwake == expected.IsAwake
                    && actual.SleepFrames == expected.SleepFrames,
                    "restored body state " + expected.Id);
            }

            for (int step = 0; step < 300; step++)
            {
                second.Step();
            }

            Equal(finalChecksum, second.ComputeChecksum(), "replay after restore must match the original run");
        }

        private static CYGPhysics2DWorld BuildDeterminismScene()
        {
            CYGPhysics2DWorld world = new CYGPhysics2DWorld(new FPVector2(0, -10));

            world.AddBody(CYGPhysics2DBody.Box(
                new FPVector2(0, 0),
                new FPVector2(10, F(1, 2)),
                CYGPhysics2DBodyType.Static,
                FP.One));
            world.AddBody(CYGPhysics2DBody.Box(
                new FPVector2(-8, 4),
                new FPVector2(F(1, 2), 4),
                CYGPhysics2DBodyType.Static,
                FP.One));
            world.AddBody(CYGPhysics2DBody.Box(
                new FPVector2(8, 4),
                new FPVector2(F(1, 2), 4),
                CYGPhysics2DBodyType.Static,
                FP.One));

            CYGPhysics2DBody trigger = CYGPhysics2DBody.Circle(
                new FPVector2(0, F(3, 2)),
                F(3, 4),
                CYGPhysics2DBodyType.Static,
                FP.One);
            trigger.IsTrigger = true;
            world.AddBody(trigger);

            for (int row = 0; row < 4; row++)
            {
                for (int column = 0; column < 7; column++)
                {
                    int index = row * 7 + column;
                    FPVector2 position = new FPVector2((column - 3) * 2, 3 + row * 2);
                    FP mass = F(1 + (index % 3), 2);
                    int id;
                    if ((index % 2) == 0)
                    {
                        id = world.AddBody(CYGPhysics2DBody.Box(
                            position,
                            new FPVector2(F(2, 5), F(2, 5)),
                            F((index % 5) - 2, 20),
                            CYGPhysics2DBodyType.Dynamic,
                            mass));
                    }
                    else
                    {
                        id = world.AddBody(CYGPhysics2DBody.Circle(
                            position,
                            F(2, 5),
                            CYGPhysics2DBodyType.Dynamic,
                            mass));
                    }

                    world.SetMaterial(id, F(1, 2), ((index % 5) == 0) ? F(1, 4) : FP.Zero);
                }
            }

            int distanceAnchor = AddStaticCircle(world, new FPVector2(-3, 14), F(1, 5));
            int distanceBob = world.AddBody(CYGPhysics2DBody.Circle(new FPVector2(-3, 12), F(1, 5)));
            world.CreateDistanceJoint(distanceAnchor, distanceBob);
            world.SetVelocity(distanceBob, new FPVector2(2, 0));

            int springAnchor = AddStaticCircle(world, new FPVector2(5, 14), F(1, 5));
            int springBob = world.AddBody(CYGPhysics2DBody.Circle(new FPVector2(5, 12), F(1, 5)));
            world.CreateSpringJoint(springAnchor, springBob, F(3, 2), FP.FromInt(5), FP.One);

            return world;
        }

        //---------------------------------------------------------------------
        // Allocation-free stepping
        //---------------------------------------------------------------------
        private static void TestNoStepAllocations()
        {
            CYGPhysics2DWorld world = new CYGPhysics2DWorld(new FPVector2(0, -10));
            world.AddBody(CYGPhysics2DBody.Box(
                new FPVector2(0, 0),
                new FPVector2(8, F(1, 2)),
                CYGPhysics2DBodyType.Static,
                FP.One));
            world.AddBody(CYGPhysics2DBody.Box(
                new FPVector2(-6, 4),
                new FPVector2(F(1, 2), 2),
                CYGPhysics2DBodyType.Static,
                FP.One));
            world.AddBody(CYGPhysics2DBody.Box(
                new FPVector2(6, 4),
                new FPVector2(F(1, 2), 2),
                CYGPhysics2DBodyType.Static,
                FP.One));

            for (int i = 0; i < 10; i++)
            {
                world.AddBody(CYGPhysics2DBody.Circle(
                    new FPVector2(-3 + i * F(2, 3), 2 + i),
                    F(1, 4)));
            }

            for (int step = 0; step < 60; step++)
            {
                world.Step();
            }

            long before = GC.GetAllocatedBytesForCurrentThread();
            for (int step = 0; step < 30; step++)
            {
                world.Step();
            }

            long allocated = GC.GetAllocatedBytesForCurrentThread() - before;
            Require(allocated == 0, "Step should not allocate, allocated " + allocated + " bytes");
        }

        //---------------------------------------------------------------------
        // Helpers
        //---------------------------------------------------------------------
        private static int AddStaticCircle(CYGPhysics2DWorld world, FPVector2 position, FP radius)
        {
            return world.AddBody(CYGPhysics2DBody.Circle(
                position,
                radius,
                CYGPhysics2DBodyType.Static,
                FP.One));
        }

        private static int AddStaticBox(CYGPhysics2DWorld world, FPVector2 position, FPVector2 halfExtents)
        {
            return world.AddBody(CYGPhysics2DBody.Box(
                position,
                halfExtents,
                CYGPhysics2DBodyType.Static,
                FP.One));
        }

        private static FP F(long numerator, long denominator)
        {
            return FP.FromFraction(numerator, denominator);
        }

        private static void Require(bool condition, string label)
        {
            if (!condition)
            {
                throw new InvalidOperationException(label);
            }

            _assertions++;
        }

        private static void Equal(int expected, int actual, string label)
        {
            if (expected != actual)
            {
                throw new InvalidOperationException(label + ": expected " + expected + ", got " + actual);
            }

            _assertions++;
        }

        private static void Equal(uint expected, uint actual, string label)
        {
            if (expected != actual)
            {
                throw new InvalidOperationException(
                    label + ": expected 0x" + expected.ToString("X8") + ", got 0x" + actual.ToString("X8"));
            }

            _assertions++;
        }

        private static void Near(FP actual, FP expected, FP tolerance, string label)
        {
            FP difference = FP.Abs(actual - expected);
            if (difference > tolerance)
            {
                throw new InvalidOperationException(
                    label + ": expected " + expected.RawValue + " +/- " + tolerance.RawValue
                    + ", got " + actual.RawValue);
            }

            _assertions++;
        }
    }
}
