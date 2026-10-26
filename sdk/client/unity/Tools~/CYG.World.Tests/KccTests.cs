using System;

namespace CookYourGame.Sim.Tests
{
    internal static class KccTests
    {
        public static int AssertionCount
        {
            get { return _assertions; }
        }

        public static void RunAll()
        {
            TestGroundContact();
            TestWallContact();
            TestCeilingContact();
            TestStepClimb();
            TestMovingPlatformCarry();
        }

        private static void TestGroundContact()
        {
            SimpleCollisionWorld world = FlatGround();
            FPCharacterController controller = NewController(FP.FromInt(1));

            controller.Move(FPVector3.Down * FP.FromInt(1), world);

            Require(controller.IsGrounded, "controller lands on flat ground");
            Require(Has(controller.CollisionFlags, CYGCollisionFlags.Below), "ground contact sets Below");
            Require(controller.Position.y < (FP)0.1f, "controller settles near the floor");
        }

        private static void TestWallContact()
        {
            SimpleCollisionWorld world = FlatGround();
            world.AddBox(Box(2, 0, -10, 3, 3, 10));
            FPCharacterController controller = NewController((FP)0.02f);
            controller.CheckGrounded(world);

            controller.Move(FPVector3.Right * FP.FromInt(3), world);

            Require(Has(controller.CollisionFlags, CYGCollisionFlags.Sides), "wall contact sets Sides");
            Require(!Has(controller.CollisionFlags, CYGCollisionFlags.Above), "wall contact must not set Above");
            Require(controller.Position.x < (FP)1.6f, "wall blocks the controller: " + controller.Position.x);
        }

        private static void TestCeilingContact()
        {
            SimpleCollisionWorld world = FlatGround();
            world.AddBox(Box(-10, 3, -10, 10, 4, 10));
            FPCharacterController controller = NewController((FP)0.02f);
            controller.CheckGrounded(world);

            controller.Move(FPVector3.Up * FP.FromInt(3), world);

            Require(Has(controller.CollisionFlags, CYGCollisionFlags.Above), "ceiling contact sets Above");
            Require(controller.Position.y < (FP)1.1f, "ceiling blocks the controller: " + controller.Position.y);
        }

        private static void TestStepClimb()
        {
            SimpleCollisionWorld world = FlatGround();
            world.AddBox(Box(1, 0, -10, 2, (FP)0.25f, 10));
            FPCharacterController controller = NewController((FP)0.02f);
            controller.CheckGrounded(world);

            controller.Move(FPVector3.Right * FP.FromInt(2), world);

            Require(controller.Position.x > FP.One, "controller advances onto the step: " + controller.Position.x);
            Require(controller.Position.y > (FP)0.2f, "controller climbs the step: " + controller.Position.y);
            Require(controller.IsGrounded, "controller is grounded on the step");
        }

        private static void TestMovingPlatformCarry()
        {
            SimpleCollisionWorld world = FlatGround();
            world.AddMovingPlatform(new CYGMovingPlatform(
                1,
                Box(-5, 0, -5, 5, (FP)0.5f, 5),
                FPVector3.Right));

            FPCharacterController controller = NewController((FP)0.5f);
            controller.CheckGrounded(world);
            Require(controller.IsGrounded, "controller starts grounded on the platform");
            Require(controller.PlatformVelocity.x == FP.One, "platform velocity is exposed");

            FP dt = FP.FromFloat(0.1f);
            for (int i = 0; i < 5; i++)
            {
                world.AdvancePlatforms(dt);
                controller.SimpleMove(FPVector3.Zero, dt, world, FP.Zero);
            }

            Require(controller.Position.x > (FP)0.4f, "controller is carried by the platform: " + controller.Position.x);
            Require(controller.IsGrounded, "controller stays grounded on the platform");
        }

        private static SimpleCollisionWorld FlatGround()
        {
            SimpleCollisionWorld world = new SimpleCollisionWorld();
            world.AddBox(Box(-10, -1, -10, 10, 0, 10));
            return world;
        }

        private static FPCharacterController NewController(FP y)
        {
            return new FPCharacterController(
                new FPVector3(FP.Zero, y, FP.Zero),
                (FP)0.5f,
                FP.FromInt(2));
        }

        private static FPAABB Box(int minX, int minY, int minZ, int maxX, int maxY, int maxZ)
        {
            return new FPAABB(
                new FPVector3(FP.FromInt(minX), FP.FromInt(minY), FP.FromInt(minZ)),
                new FPVector3(FP.FromInt(maxX), FP.FromInt(maxY), FP.FromInt(maxZ)));
        }

        private static FPAABB Box(int minX, FP minY, int minZ, int maxX, FP maxY, int maxZ)
        {
            return new FPAABB(
                new FPVector3(FP.FromInt(minX), minY, FP.FromInt(minZ)),
                new FPVector3(FP.FromInt(maxX), maxY, FP.FromInt(maxZ)));
        }

        private static bool Has(CYGCollisionFlags flags, CYGCollisionFlags expected)
        {
            return (flags & expected) != 0;
        }

        private static int _assertions;

        private static void Require(bool condition, string message)
        {
            _assertions++;
            if (!condition)
            {
                throw new InvalidOperationException(message);
            }
        }
    }
}
