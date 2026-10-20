//-----------------------------------------------------------------------------
// DeterminismCheckTest.cs
// Standalone C# determinism test runner.
// Runs 10,000 operations (scalars, vectors, cross products, quats, trig, raycasts)
// and computes an immutable SHA256 & CRC32 checksum string.
// Zero UnityEngine dependencies.
//-----------------------------------------------------------------------------

using System;
using System.Security.Cryptography;
using System.Text;

namespace CookYourGame.Sim
{
    public static class DeterminismCheckTest
    {
        public static void Main(string[] args)
        {
            Console.WriteLine("=================================================");
            Console.WriteLine(" CookYourGame Deterministic Simulation Test Runner");
            Console.WriteLine("=================================================");

            var result1 = RunSimulation(10000, 1337);
            Console.WriteLine($"[Pass 1] 10,000 operations completed.");
            Console.WriteLine($"[Pass 1] SHA-256 Checksum: {result1.Sha256}");
            Console.WriteLine($"[Pass 1] CRC-32  Checksum: 0x{result1.Crc32:X8}");
            Console.WriteLine($"[Pass 1] FNV-1a 64 Checksum: 0x{result1.Fnv1a64:X16}");

            var result2 = RunSimulation(10000, 1337);
            Console.WriteLine($"[Pass 2] 10,000 operations completed.");
            Console.WriteLine($"[Pass 2] SHA-256 Checksum: {result2.Sha256}");
            Console.WriteLine($"[Pass 2] CRC-32  Checksum: 0x{result2.Crc32:X8}");
            Console.WriteLine($"[Pass 2] FNV-1a 64 Checksum: 0x{result2.Fnv1a64:X16}");

            // Golden values pin the fixed-point math across machines and
            // runtimes. They are intentionally checked in: a mismatch means
            // the simulation changed, not just that two runs in this process
            // happened to agree.
            const string GoldenSha256 =
                "43544fe45821264e85e4f16a11925103d9c811ed7f4b40899ff15384aa5ed9af";
            const uint GoldenCrc32 = 0xE9D6E0FF;

            bool sameRun = result1.Sha256 == result2.Sha256
                && result1.Crc32 == result2.Crc32
                && result1.Fnv1a64 == result2.Fnv1a64;
            bool golden = result1.Sha256 == GoldenSha256
                && result1.Crc32 == GoldenCrc32
                && result1.Fnv1a64 == CYGDeterminismProbe.GoldenFnv1a64;

            if (sameRun && golden)
            {
                Console.WriteLine(">>> SUCCESS: Determinism Verified! 100% bit-for-bit match. <<<");
            }
            else
            {
                Console.ForegroundColor = ConsoleColor.Red;
                Console.WriteLine(sameRun
                    ? ">>> FAILURE: golden checksum mismatch (simulation changed). <<<"
                    : ">>> FAILURE: Determinism mismatch detected! <<<");
                Console.ResetColor();
                Environment.Exit(1);
            }

            TestCharacterController();
            RollbackSimTest.RunAllTests();
        }

        public static void TestCharacterController()
        {
            Console.WriteLine("--- Testing FPCharacterController ---");
            var world = new SimpleCollisionWorld();
            // Floor at y in [-1, 0]
            world.AddBox(new FPAABB(new FPVector3(-10, -1, -10), new FPVector3(10, 0, 10)));
            // Step at x in [2, 4], y in [0, 0.2]
            world.AddBox(new FPAABB(new FPVector3(2, 0, -2), new FPVector3(4, (FP)0.2f, 2)));
            // Obstacle wall at x in [5, 6], y in [0, 3]
            world.AddBox(new FPAABB(new FPVector3(5, 0, -5), new FPVector3(6, 3, 5)));

            var cc = new FPCharacterController(new FPVector3(0, 0, 0), (FP)0.5f, (FP)2.0f);
            cc.StepOffset = (FP)0.3f;

            // 1. Initial Ground Check
            cc.CheckGrounded(world);
            if (!cc.IsGrounded) throw new Exception("Character controller should be grounded on floor!");
            Console.WriteLine($"[CC Check 1] Grounded on floor: {cc.IsGrounded}, Normal: {cc.GroundNormal}");

            // 2. Walk forward and step onto step. A grounded capsule used to
            // report a zero-distance floor hit and never move horizontally.
            cc.Move(new FPVector3((FP)2.5f, 0, 0), world);
            Console.WriteLine($"[CC Check 2] Position after step move: {cc.Position}, Grounded: {cc.IsGrounded}");
            if (cc.Position.x < (FP)2.4f || cc.Position.x > (FP)2.6f)
                throw new Exception($"Horizontal movement failed: x={cc.Position.x}");
            if (cc.Position.y < (FP)0.18f)
                throw new Exception($"Step climb failed: y={cc.Position.y}");
            if (!cc.IsGrounded)
                throw new Exception("Character should be grounded after stepping.");

            // 3. Move towards wall
            cc.Move(new FPVector3((FP)5.0f, 0, 0), world);
            Console.WriteLine($"[CC Check 3] Position after moving towards wall: {cc.Position}");
            if (cc.Position.x >= (FP)5.0f) throw new Exception("Character controller walked through wall!");
            if (cc.Position.x < (FP)4.0f) throw new Exception($"Character stopped short of wall: x={cc.Position.x}");

            // 4. Kinematic SimpleMove with gravity in mid-air
            cc.Position = new FPVector3(0, 5, 0);
            cc.CheckGrounded(world);
            if (cc.IsGrounded) throw new Exception("Character should not be grounded in air!");
            for (int i = 0; i < 20; i++)
            {
                cc.SimpleMove(FPVector3.Zero, (FP)0.05f, world, (FP)9.8f);
            }
            Console.WriteLine($"[CC Check 4] Landed after falling: Position={cc.Position}, Grounded={cc.IsGrounded}");
            if (!cc.IsGrounded) throw new Exception("Character should have landed on floor!");
            if (cc.Position.y < (FP)(-0.01f) || cc.Position.y > (FP)0.05f)
                throw new Exception($"Character did not settle on the floor: y={cc.Position.y}");

            Console.WriteLine(">>> SUCCESS: FPCharacterController tests passed! <<<");
        }

        public struct ChecksumResult
        {
            public string Sha256;
            public uint Crc32;
            public ulong Fnv1a64;
        }

        public static ChecksumResult RunSimulation(int iterations, uint seed)
        {
            byte[] bytes = CYGDeterminismProbe.CreateStream(iterations, seed, out var report);

            using var sha = SHA256.Create();
            byte[] hash = sha.ComputeHash(bytes);
            StringBuilder sb = new StringBuilder(hash.Length * 2);
            for (int b = 0; b < hash.Length; b++)
            {
                sb.Append(hash[b].ToString("x2"));
            }

            return new ChecksumResult
            {
                Sha256 = sb.ToString(),
                Crc32 = report.Crc32,
                Fnv1a64 = report.Fnv1a64
            };
        }
    }
}
