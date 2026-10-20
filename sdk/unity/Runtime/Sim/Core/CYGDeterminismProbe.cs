//-----------------------------------------------------------------------------
// CYGDeterminismProbe.cs
// Platform-independent golden-vector probe for the deterministic simulation.
// Runs the same mixed fixed-point workload on every runtime/device and reduces
// the raw results to CRC32 + FNV-1a 64. A mismatch means the math (or the
// build) is not bit-identical, even if a local run-to-run comparison passed.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    public readonly struct CYGDeterminismReport
    {
        public readonly uint Crc32;
        public readonly ulong Fnv1a64;
        public readonly int Iterations;
        public readonly uint Seed;

        public CYGDeterminismReport(uint crc32, ulong fnv1a64, int iterations, uint seed)
        {
            Crc32 = crc32;
            Fnv1a64 = fnv1a64;
            Iterations = iterations;
            Seed = seed;
        }
    }

    /// <summary>
    /// Canonical fixed-point workload used to prove that two builds produce the
    /// same raw values. Only integer operations are used for the stream and the
    /// hashes; the report can be logged from any Unity player backend.
    /// </summary>
    public static class CYGDeterminismProbe
    {
        public const int DefaultIterations = 10000;
        public const uint DefaultSeed = 1337;

        // Pinned from the canonical run. Update only when the simulation is
        // intentionally changed, never to make a failing device pass.
        public const uint GoldenCrc32 = 0xE9D6E0FF;
        public const ulong GoldenFnv1a64 = 0x76E67011297DCBA1UL;

        private const ulong FnvOffset = 14695981039346656037UL;
        private const ulong FnvPrime = 1099511628211UL;

        private static readonly uint[] CrcTable = BuildCrcTable();

        /// <summary>Runs the workload without allocating the result stream.</summary>
        public static CYGDeterminismReport Run(int iterations = DefaultIterations, uint seed = DefaultSeed)
        {
            return RunCore(iterations, seed, null);
        }

        /// <summary>
        /// Runs the workload and returns the exact little-endian byte stream.
        /// Two devices can diff this file when the hashes disagree.
        /// </summary>
        public static byte[] CreateStream(int iterations, uint seed, out CYGDeterminismReport report)
        {
            ByteSink sink = new ByteSink(iterations * 360);
            report = RunCore(iterations, seed, sink);
            return sink.ToArray();
        }

        /// <summary>
        /// Compares this build against the pinned golden vector. Safe to call
        /// from a Unity player on any backend.
        /// </summary>
        public static bool Verify(out string detail)
        {
            return Verify(DefaultIterations, DefaultSeed, out detail);
        }

        public static bool Verify(int iterations, uint seed, out string detail)
        {
            CYGDeterminismReport report = Run(iterations, seed);
            bool crcOk = report.Crc32 == GoldenCrc32;
            bool fnvOk = report.Fnv1a64 == GoldenFnv1a64;

            detail = $"crc=0x{report.Crc32:X8} fnv=0x{report.Fnv1a64:X16} " +
                     $"iterations={report.Iterations} seed={report.Seed}";

            if (crcOk && fnvOk)
            {
                detail += " golden=match";
                return true;
            }

            detail += $" golden=crc:0x{GoldenCrc32:X8}/fnv:0x{GoldenFnv1a64:X16} MISMATCH";
            return false;
        }

        private static CYGDeterminismReport RunCore(int iterations, uint seed, ByteSink sink)
        {
            uint crc = 0xFFFFFFFFu;
            ulong fnv = FnvOffset;
            uint lcg = seed;

            uint Next()
            {
                lcg = unchecked(lcg * 1664525u + 1013904223u);
                return lcg;
            }

            for (int i = 0; i < iterations; i++)
            {
                long rawA = (long)((Next() % 2000000) - 1000000) << 16;
                long rawB = (long)((Next() % 2000000) - 1000000) << 16;
                long rawC = (long)((Next() % 2000000) - 1000000) << 16;

                if (rawB == 0) rawB = Fixed64Math.One;

                FP a = FP.FromRaw(rawA);
                FP b = FP.FromRaw(rawB);
                FP c = FP.FromRaw(rawC);

                FP sum = a + b;
                FP diff = a - b;
                FP mul = a * b;
                FP div = a / b;
                FP mod = a % b;
                FP abs = FP.Abs(a);
                FP sqrt = FP.Sqrt(abs);

                FP sinA = FP.Sin(a);
                FP cosB = FP.Cos(b);
                FP atan2AB = FP.Atan2(a, b);

                FPVector2 v2a = new FPVector2(a, b);
                FPVector2 v2b = new FPVector2(b, c);
                FP v2dot = FPVector2.Dot(v2a, v2b);
                FPVector2 v2lerp = FPVector2.Lerp(v2a, v2b, FP.Half);
                FPVector2 v2norm = v2a.Normalized;

                FPVector3 v3a = new FPVector3(a, b, c);
                FPVector3 v3b = new FPVector3(b, c, a);
                FP v3dot = FPVector3.Dot(v3a, v3b);
                FPVector3 v3cross = FPVector3.Cross(v3a, v3b);
                FP v3dist = FPVector3.Distance(v3a, v3b);
                FPVector3 v3proj = FPVector3.ProjectOnPlane(v3a, FPVector3.Up);
                FPVector3 v3norm = v3cross.Normalized;

                FPQuaternion q1 = FPQuaternion.Euler(FP.Abs(a) % FP.FromInt(360), FP.Abs(b) % FP.FromInt(360), FP.Abs(c) % FP.FromInt(360));
                FPVector3 axis = v3norm != FPVector3.Zero ? v3norm : FPVector3.Up;
                FPQuaternion q2 = FPQuaternion.FromAxisAngle(axis, a);
                FPQuaternion qMul = q1 * q2;
                FPQuaternion qSlerp = FPQuaternion.Slerp(q1, q2, FP.Half);
                FPVector3 rotated = qMul * v3a;

                FPVector3 boxHalf = new FPVector3(FP.Abs(a) + FP.One, FP.Abs(b) + FP.One, FP.Abs(c) + FP.One);
                FPAABB aabb = new FPAABB(v3a - boxHalf, v3a + boxHalf);
                FPSphere sphere = new FPSphere(v3b, FP.Abs(a) + FP.One);
                FPCapsule capsule = FPCapsule.CreateVertical(v3a, FP.Abs(b) + FP.FromInt(2), FP.Half);

                bool aabbContains = aabb.Contains(v3b);
                bool aabbSphereIntersects = aabb.Intersects(sphere);
                bool capsuleSphereIntersects = capsule.Intersects(sphere);

                FPRay ray = new FPRay(v3b + FPVector3.Up * FP.FromInt(10), FPVector3.Down);
                bool hitAABB = FPRaycast.RaycastAABB(ray, aabb, FP.FromInt(100), out var hAABB);
                bool hitSphere = FPRaycast.RaycastSphere(ray, sphere, FP.FromInt(100), out var hSphere);
                bool hitCapsule = FPRaycast.RaycastCapsule(ray, capsule, FP.FromInt(100), out var hCapsule);

                bool sCast = FPRaycast.SphereCast(new FPSphere(ray.Origin, FP.FromRaw(1L << 30)), ray.Direction, FP.FromInt(100), aabb, out var hCast);

                MixLong(sink, ref crc, ref fnv, sum.RawValue);
                MixLong(sink, ref crc, ref fnv, diff.RawValue);
                MixLong(sink, ref crc, ref fnv, mul.RawValue);
                MixLong(sink, ref crc, ref fnv, div.RawValue);
                MixLong(sink, ref crc, ref fnv, mod.RawValue);
                MixLong(sink, ref crc, ref fnv, sqrt.RawValue);
                MixLong(sink, ref crc, ref fnv, sinA.RawValue);
                MixLong(sink, ref crc, ref fnv, cosB.RawValue);
                MixLong(sink, ref crc, ref fnv, atan2AB.RawValue);
                MixLong(sink, ref crc, ref fnv, v2dot.RawValue);
                MixLong(sink, ref crc, ref fnv, v2lerp.x.RawValue);
                MixLong(sink, ref crc, ref fnv, v2lerp.y.RawValue);
                MixLong(sink, ref crc, ref fnv, v3dot.RawValue);
                MixLong(sink, ref crc, ref fnv, v3cross.x.RawValue);
                MixLong(sink, ref crc, ref fnv, v3cross.y.RawValue);
                MixLong(sink, ref crc, ref fnv, v3cross.z.RawValue);
                MixLong(sink, ref crc, ref fnv, v3dist.RawValue);
                MixLong(sink, ref crc, ref fnv, v3proj.x.RawValue);
                MixLong(sink, ref crc, ref fnv, v3proj.y.RawValue);
                MixLong(sink, ref crc, ref fnv, v3proj.z.RawValue);
                MixLong(sink, ref crc, ref fnv, qMul.x.RawValue);
                MixLong(sink, ref crc, ref fnv, qMul.y.RawValue);
                MixLong(sink, ref crc, ref fnv, qMul.z.RawValue);
                MixLong(sink, ref crc, ref fnv, qMul.w.RawValue);
                MixLong(sink, ref crc, ref fnv, qSlerp.x.RawValue);
                MixLong(sink, ref crc, ref fnv, qSlerp.y.RawValue);
                MixLong(sink, ref crc, ref fnv, qSlerp.z.RawValue);
                MixLong(sink, ref crc, ref fnv, qSlerp.w.RawValue);
                MixLong(sink, ref crc, ref fnv, rotated.x.RawValue);
                MixLong(sink, ref crc, ref fnv, rotated.y.RawValue);
                MixLong(sink, ref crc, ref fnv, rotated.z.RawValue);
                MixBool(sink, ref crc, ref fnv, aabbContains);
                MixBool(sink, ref crc, ref fnv, aabbSphereIntersects);
                MixBool(sink, ref crc, ref fnv, capsuleSphereIntersects);
                MixBool(sink, ref crc, ref fnv, hitAABB);
                if (hitAABB)
                {
                    MixLong(sink, ref crc, ref fnv, hAABB.Distance.RawValue);
                    MixLong(sink, ref crc, ref fnv, hAABB.Normal.x.RawValue);
                }
                MixBool(sink, ref crc, ref fnv, hitSphere);
                if (hitSphere)
                {
                    MixLong(sink, ref crc, ref fnv, hSphere.Distance.RawValue);
                    MixLong(sink, ref crc, ref fnv, hSphere.Normal.y.RawValue);
                }
                MixBool(sink, ref crc, ref fnv, hitCapsule);
                if (hitCapsule)
                {
                    MixLong(sink, ref crc, ref fnv, hCapsule.Distance.RawValue);
                    MixLong(sink, ref crc, ref fnv, hCapsule.Normal.z.RawValue);
                }
                MixBool(sink, ref crc, ref fnv, sCast);
                if (sCast)
                {
                    MixLong(sink, ref crc, ref fnv, hCast.Distance.RawValue);
                }
            }

            return new CYGDeterminismReport(crc ^ 0xFFFFFFFFu, fnv, iterations, seed);
        }

        private static void MixLong(ByteSink sink, ref uint crc, ref ulong fnv, long value)
        {
            for (int i = 0; i < 8; i++)
            {
                MixByte(sink, ref crc, ref fnv, (byte)(value >> (i * 8)));
            }
        }

        private static void MixBool(ByteSink sink, ref uint crc, ref ulong fnv, bool value)
        {
            MixByte(sink, ref crc, ref fnv, value ? (byte)1 : (byte)0);
        }

        private static void MixByte(ByteSink sink, ref uint crc, ref ulong fnv, byte value)
        {
            crc = CrcTable[(crc ^ value) & 0xFF] ^ (crc >> 8);
            fnv ^= value;
            fnv = unchecked(fnv * FnvPrime);
            sink?.WriteByte(value);
        }

        private static uint[] BuildCrcTable()
        {
            const uint polynomial = 0xEDB88320u;
            uint[] table = new uint[256];
            for (uint i = 0; i < 256; i++)
            {
                uint entry = i;
                for (int j = 0; j < 8; j++)
                {
                    entry = (entry & 1) == 1 ? (entry >> 1) ^ polynomial : entry >> 1;
                }
                table[i] = entry;
            }
            return table;
        }

        private sealed class ByteSink
        {
            private byte[] _buffer;
            private int _length;

            public ByteSink(int capacity)
            {
                _buffer = new byte[capacity > 0 ? capacity : 64];
            }

            public void WriteByte(byte value)
            {
                if (_length == _buffer.Length)
                {
                    Array.Resize(ref _buffer, _buffer.Length * 2);
                }
                _buffer[_length++] = value;
            }

            public byte[] ToArray()
            {
                if (_length == _buffer.Length)
                {
                    return _buffer;
                }
                byte[] result = new byte[_length];
                Buffer.BlockCopy(_buffer, 0, result, 0, _length);
                return result;
            }
        }
    }
}
