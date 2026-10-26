using System;
using CookYourGame.Sim;

namespace CYG.CodeGen.Tests
{
    internal static class Program
    {
        private static int Main()
        {
            CharacterState original = new CharacterState
            {
                Health = FP.FromInt(100),
                Velocity = new FPVector3(FP.FromInt(1), FP.FromInt(-2), FP.FromInt(3)),
                Grounded = true
            };

            Require(CharacterState.SizeInBytes == 33, "expected SizeInBytes 33, got " + CharacterState.SizeInBytes);
            Require(CharacterState.TypeId == 1001, "expected TypeId 1001");
            Require(CharacterState.SchemaHash != 0, "expected non-zero schema hash");

            byte[] buffer = new byte[CharacterState.SizeInBytes + 7];
            int written = original.WriteTo(buffer, 3);
            Require(written == CharacterState.SizeInBytes, "WriteTo returned wrong size");

            CharacterState restored = new CharacterState();
            restored.ReadFrom(buffer, 3);
            Require(restored.Health.RawValue == original.Health.RawValue, "health round-trip failed");
            Require(restored.Velocity.x.RawValue == original.Velocity.x.RawValue, "velocity.x round-trip failed");
            Require(restored.Velocity.y.RawValue == original.Velocity.y.RawValue, "velocity.y round-trip failed");
            Require(restored.Velocity.z.RawValue == original.Velocity.z.RawValue, "velocity.z round-trip failed");
            Require(restored.Grounded == original.Grounded, "bool round-trip failed");

            uint checksumA = 0xFFFFFFFFu;
            uint checksumB = 0xFFFFFFFFu;
            original.ComputeChecksum(ref checksumA);
            restored.ComputeChecksum(ref checksumB);
            Require(checksumA == checksumB, "checksum mismatch after round-trip");

            CharacterState changed = restored;
            changed.Grounded = false;
            uint checksumC = 0xFFFFFFFFu;
            changed.ComputeChecksum(ref checksumC);
            Require(checksumA != checksumC, "checksum did not change when state changed");

            CharacterState copy = new CharacterState();
            original.CopyTo(ref copy);
            Require(copy.Health.RawValue == original.Health.RawValue, "CopyTo failed");

            bool threw = false;
            try
            {
                restored.ReadFrom(new byte[CharacterState.SizeInBytes - 1], 0);
            }
            catch (ArgumentOutOfRangeException)
            {
                threw = true;
            }
            Require(threw, "short buffer should throw");

            TestPrimitives();
            TestKeywordIdentifiers();
            TestGlobalNamespace();

            Console.WriteLine("CYG codegen test passed: size=" + CharacterState.SizeInBytes
                + " typeId=" + CharacterState.TypeId
                + " schema=0x" + CharacterState.SchemaHash.ToString("X8")
                + " checksum=0x" + checksumA.ToString("X8"));
            return 0;
        }

        private static void TestPrimitives()
        {
            PrimitiveState original = new PrimitiveState
            {
                A = 0xAB,
                B = -7,
                C = -1234,
                D = 54321,
                E = -123456789,
                F = 0xDEADBEEFu,
                G = -1234567890123456789L,
                H = 0xFEDCBA9876543210UL,
                I = '\u20AC',
                J = true,
                K = TestMode.Minus
            };

            Require(PrimitiveState.SizeInBytes == 34, "expected PrimitiveState size 34, got " + PrimitiveState.SizeInBytes);
            byte[] buffer = new byte[PrimitiveState.SizeInBytes];
            Require(original.WriteTo(buffer, 0) == PrimitiveState.SizeInBytes, "primitive WriteTo size");

            PrimitiveState restored = new PrimitiveState();
            restored.ReadFrom(buffer, 0);
            Require(restored.A == original.A && restored.B == original.B && restored.C == original.C, "primitive integer round-trip 1");
            Require(restored.D == original.D && restored.E == original.E && restored.F == original.F, "primitive integer round-trip 2");
            Require(restored.G == original.G && restored.H == original.H, "primitive integer round-trip 3");
            Require(restored.I == original.I && restored.J == original.J && restored.K == original.K, "primitive char/bool/enum round-trip");

            uint checksum = 0xFFFFFFFFu;
            restored.ComputeChecksum(ref checksum);
            restored.K = TestMode.Plus;
            uint changed = 0xFFFFFFFFu;
            restored.ComputeChecksum(ref changed);
            Require(checksum != changed, "primitive enum checksum did not change");
        }

        private static void TestKeywordIdentifiers()
        {
            KeywordState original = new KeywordState { @class = 123, @event = true };
            Require(KeywordState.SizeInBytes == 5, "expected KeywordState size 5");
            byte[] buffer = new byte[KeywordState.SizeInBytes];
            original.WriteTo(buffer, 0);
            KeywordState restored = new KeywordState();
            restored.ReadFrom(buffer, 0);
            Require(restored.@class == original.@class && restored.@event == original.@event, "keyword identifier round-trip failed");
        }

        private static void TestGlobalNamespace()
        {
            GlobalNamespaceState original = new GlobalNamespaceState { Value = FP.FromInt(12), Flag = true };
            Require(GlobalNamespaceState.SizeInBytes == 9, "expected GlobalNamespaceState size 9");
            Require(GlobalNamespaceState.TypeId == 1004, "expected GlobalNamespaceState type id 1004");
            byte[] buffer = new byte[GlobalNamespaceState.SizeInBytes];
            original.WriteTo(buffer, 0);
            GlobalNamespaceState restored = new GlobalNamespaceState();
            restored.ReadFrom(buffer, 0);
            Require(restored.Value.RawValue == original.Value.RawValue && restored.Flag, "global namespace component round-trip failed");
        }

        private static void Require(bool condition, string message)
        {
            if (!condition)
            {
                throw new InvalidOperationException(message);
            }
        }
    }
}

[CYGComponent(Id = 1004)]
public partial struct GlobalNamespaceState
{
    public FP Value;
    public bool Flag;
}
