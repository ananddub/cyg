//-----------------------------------------------------------------------------
// DeterminismCheckTest.cs
// Standalone C# determinism test runner.
// Runs 10,000 operations (scalars, vectors, cross products, quats, trig, raycasts)
// and computes an immutable SHA256 & CRC32 checksum string.
// Zero UnityEngine dependencies.
//-----------------------------------------------------------------------------

using System;
using System.IO;
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

            var result2 = RunSimulation(10000, 1337);
            Console.WriteLine($"[Pass 2] 10,000 operations completed.");
            Console.WriteLine($"[Pass 2] SHA-256 Checksum: {result2.Sha256}");
            Console.WriteLine($"[Pass 2] CRC-32  Checksum: 0x{result2.Crc32:X8}");

            // Golden values pin the fixed-point math across machines and
            // runtimes. They are intentionally checked in: a mismatch means
            // the simulation changed, not just that two runs in this process
            // happened to agree.
            const string GoldenSha256 =
                "43544fe45821264e85e4f16a11925103d9c811ed7f4b40899ff15384aa5ed9af";
            const uint GoldenCrc32 = 0xE9D6E0FF;

            bool sameRun = result1.Sha256 == result2.Sha256
                && result1.Crc32 == result2.Crc32;
            bool golden = result1.Sha256 == GoldenSha256
                && result1.Crc32 == GoldenCrc32;

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
        }

        public static ChecksumResult RunSimulation(int iterations, uint seed)
        {
            using var sha = SHA256.Create();
            using var ms = new MemoryStream();
            using var writer = new BinaryWriter(ms);

            uint lcg = seed;
            uint Next()
            {
                lcg = lcg * 1664525u + 1013904223u;
                return lcg;
            }

            for (int i = 0; i < iterations; i++)
            {
                // Generate pseudo-random fixed points in reasonable ranges
                long rawA = (long)((Next() % 2000000) - 1000000) << 16;
                long rawB = (long)((Next() % 2000000) - 1000000) << 16;
                long rawC = (long)((Next() % 2000000) - 1000000) << 16;

                if (rawB == 0) rawB = Fixed64Math.One;

                FP a = FP.FromRaw(rawA);
                FP b = FP.FromRaw(rawB);
                FP c = FP.FromRaw(rawC);

                // 1. Basic Scalar Arithmetic
                FP sum = a + b;
                FP diff = a - b;
                FP mul = a * b;
                FP div = a / b;
                FP mod = a % b;
                FP abs = FP.Abs(a);
                FP sqrt = FP.Sqrt(abs);

                // 2. Trigonometry
                FP sinA = FP.Sin(a);
                FP cosB = FP.Cos(b);
                FP atan2AB = FP.Atan2(a, b);

                // 3. 2D Vectors
                FPVector2 v2a = new FPVector2(a, b);
                FPVector2 v2b = new FPVector2(b, c);
                FP v2dot = FPVector2.Dot(v2a, v2b);
                FPVector2 v2lerp = FPVector2.Lerp(v2a, v2b, (FP)0.5f);
                FPVector2 v2norm = v2a.Normalized;

                // 4. 3D Vectors
                FPVector3 v3a = new FPVector3(a, b, c);
                FPVector3 v3b = new FPVector3(b, c, a);
                FP v3dot = FPVector3.Dot(v3a, v3b);
                FPVector3 v3cross = FPVector3.Cross(v3a, v3b);
                FP v3dist = FPVector3.Distance(v3a, v3b);
                FPVector3 v3proj = FPVector3.ProjectOnPlane(v3a, FPVector3.Up);
                FPVector3 v3norm = v3cross.Normalized;

                // 5. 3D Quaternions
                FPQuaternion q1 = FPQuaternion.Euler(FP.Abs(a) % (FP)360, FP.Abs(b) % (FP)360, FP.Abs(c) % (FP)360);
                FPVector3 axis = v3norm != FPVector3.Zero ? v3norm : FPVector3.Up;
                FPQuaternion q2 = FPQuaternion.FromAxisAngle(axis, a);
                FPQuaternion qMul = q1 * q2;
                FPQuaternion qSlerp = FPQuaternion.Slerp(q1, q2, (FP)0.5f);
                FPVector3 rotated = qMul * v3a;

                // 6. Geometry & Collisions
                FPVector3 boxHalf = new FPVector3(FP.Abs(a) + FP.One, FP.Abs(b) + FP.One, FP.Abs(c) + FP.One);
                FPAABB aabb = new FPAABB(v3a - boxHalf, v3a + boxHalf);
                FPSphere sphere = new FPSphere(v3b, FP.Abs(a) + FP.One);
                FPCapsule capsule = FPCapsule.CreateVertical(v3a, FP.Abs(b) + (FP)2, (FP)0.5f);

                bool aabbContains = aabb.Contains(v3b);
                bool aabbSphereIntersects = aabb.Intersects(sphere);
                bool capsuleSphereIntersects = capsule.Intersects(sphere);

                // 7. Raycast & Shapecast Queries
                FPRay ray = new FPRay(v3b + FPVector3.Up * (FP)10, FPVector3.Down);
                bool hitAABB = FPRaycast.RaycastAABB(ray, aabb, (FP)100, out var hAABB);
                bool hitSphere = FPRaycast.RaycastSphere(ray, sphere, (FP)100, out var hSphere);
                bool hitCapsule = FPRaycast.RaycastCapsule(ray, capsule, (FP)100, out var hCapsule);

                bool sCast = FPRaycast.SphereCast(new FPSphere(ray.Origin, (FP)0.25f), ray.Direction, (FP)100, aabb, out var hCast);

                // Write all results to the deterministic stream
                writer.Write(sum.RawValue);
                writer.Write(diff.RawValue);
                writer.Write(mul.RawValue);
                writer.Write(div.RawValue);
                writer.Write(mod.RawValue);
                writer.Write(sqrt.RawValue);
                writer.Write(sinA.RawValue);
                writer.Write(cosB.RawValue);
                writer.Write(atan2AB.RawValue);
                writer.Write(v2dot.RawValue);
                writer.Write(v2lerp.x.RawValue);
                writer.Write(v2lerp.y.RawValue);
                writer.Write(v3dot.RawValue);
                writer.Write(v3cross.x.RawValue);
                writer.Write(v3cross.y.RawValue);
                writer.Write(v3cross.z.RawValue);
                writer.Write(v3dist.RawValue);
                writer.Write(v3proj.x.RawValue);
                writer.Write(v3proj.y.RawValue);
                writer.Write(v3proj.z.RawValue);
                writer.Write(qMul.x.RawValue);
                writer.Write(qMul.y.RawValue);
                writer.Write(qMul.z.RawValue);
                writer.Write(qMul.w.RawValue);
                writer.Write(qSlerp.x.RawValue);
                writer.Write(qSlerp.y.RawValue);
                writer.Write(qSlerp.z.RawValue);
                writer.Write(qSlerp.w.RawValue);
                writer.Write(rotated.x.RawValue);
                writer.Write(rotated.y.RawValue);
                writer.Write(rotated.z.RawValue);
                writer.Write(aabbContains);
                writer.Write(aabbSphereIntersects);
                writer.Write(capsuleSphereIntersects);
                writer.Write(hitAABB);
                if (hitAABB) { writer.Write(hAABB.Distance.RawValue); writer.Write(hAABB.Normal.x.RawValue); }
                writer.Write(hitSphere);
                if (hitSphere) { writer.Write(hSphere.Distance.RawValue); writer.Write(hSphere.Normal.y.RawValue); }
                writer.Write(hitCapsule);
                if (hitCapsule) { writer.Write(hCapsule.Distance.RawValue); writer.Write(hCapsule.Normal.z.RawValue); }
                writer.Write(sCast);
                if (sCast) { writer.Write(hCast.Distance.RawValue); }
            }

            byte[] bytes = ms.ToArray();
            byte[] hash = sha.ComputeHash(bytes);
            StringBuilder sb = new StringBuilder(hash.Length * 2);
            for (int b = 0; b < hash.Length; b++)
            {
                sb.Append(hash[b].ToString("x2"));
            }

            uint crc = ComputeCrc32(bytes);

            return new ChecksumResult
            {
                Sha256 = sb.ToString(),
                Crc32 = crc
            };
        }

        private static uint ComputeCrc32(byte[] bytes)
        {
            uint crc = 0xFFFFFFFF;
            for (int i = 0; i < bytes.Length; i++)
            {
                byte b = bytes[i];
                crc ^= b;
                for (int j = 0; j < 8; j++)
                {
                    uint mask = (uint)-(int)(crc & 1);
                    crc = (crc >> 1) ^ (0xEDB88320u & mask);
                }
            }
            return ~crc;
        }
    }
}
