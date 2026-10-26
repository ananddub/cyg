using CookYourGame.Sim;
using UnityEngine;

namespace CookYourGame.SportsArena
{
    /// <summary>
    /// Tuning values for the CYG-native Sports Arena Brawler sample. Values are
    /// intentionally close to the original sample so gameplay feels the
    /// same while every rule runs on CookYourGame.Sim.
    /// </summary>
    public static class SportsArenaConfig
    {
        public const int PlayerCount = 6;
        public const int TickRate = 60;

        public static readonly FP DeltaTime = FP.FromFraction(1, TickRate);
        public static readonly FPVector3 Gravity = new FPVector3(FP.Zero, FP.FromInt(-28), FP.Zero);

        public static readonly FP PlayerRadius = FP.FromFraction(2, 5);
        public static readonly FP PlayerHeight = FP.FromInt(2);
        public static readonly FP PlayerMass = FP.One;

        public static readonly FP MoveSpeed = FP.FromInt(7);
        public static readonly FP GroundAcceleration = FP.FromInt(55);
        public static readonly FP AirAcceleration = FP.FromInt(18);
        public static readonly FP JumpImpulse = FP.FromInt(10);
        public static readonly FP MaxHealth = FP.FromInt(100);

        public static readonly FP BallRadius = FP.FromFraction(1, 3);
        public static readonly FP BallMass = FP.FromFraction(1, 2);
        public static readonly FP BallRestitution = FP.FromFraction(3, 5);
        public static readonly FP BallFriction = FP.FromFraction(1, 2);
        public static readonly FP BallGravityScale = FP.FromFraction(9, 10);

        public static readonly FP PunchRange = FP.FromFraction(11, 5);
        public static readonly FP PunchRadius = FP.FromFraction(3, 5);
        public static readonly FP PunchDamage = FP.FromInt(12);
        public static readonly FP PunchCooldown = FP.FromFraction(2, 5);
        public static readonly FP PunchDuration = FP.FromFraction(1, 5);
        public static readonly FP PunchKnockback = FP.FromInt(12);

        public static readonly FP DashSpeed = FP.FromInt(18);
        public static readonly FP DashDuration = FP.FromFraction(3, 20);
        public static readonly FP DashCooldown = FP.FromFraction(6, 5);

        public static readonly FP ThrowSpeed = FP.FromInt(17);
        public static readonly FP ThrowUp = FP.FromInt(5);
        public static readonly FP ThrowCooldown = FP.FromFraction(1, 2);

        public static readonly FP CatchRadius = FP.FromFraction(6, 5);
        public static readonly FP CatchCooldown = FP.FromFraction(1, 5);
        public static readonly FP StunDuration = FP.FromFraction(2, 5);
        public static readonly FP RespawnDelay = FP.FromInt(2);

        public static readonly FP MatchDuration = FP.FromInt(300);
        public static readonly int ScoreToWin = 5;

        public static readonly FPVector3 GravityBall = new FPVector3(
            FP.Zero,
            FP.FromInt(-24) * BallGravityScale,
            FP.Zero);

        public static Vector3 ToVector(FPVector3 value)
        {
            return new Vector3(value.x.AsFloat(), value.y.AsFloat(), value.z.AsFloat());
        }

        public static FPVector3 ToFixed(Vector3 value)
        {
            return new FPVector3(
                FP.FromFloat(value.x),
                FP.FromFloat(value.y),
                FP.FromFloat(value.z));
        }

        public static Quaternion ToQuaternion(FPQuaternion value)
        {
            return new Quaternion(
                value.x.AsFloat(),
                value.y.AsFloat(),
                value.z.AsFloat(),
                value.w.AsFloat());
        }

        public static FPQuaternion ToFixed(Quaternion value)
        {
            return new FPQuaternion(
                FP.FromFloat(value.x),
                FP.FromFloat(value.y),
                FP.FromFloat(value.z),
                FP.FromFloat(value.w)).Normalized;
        }

        public static FPQuaternion Yaw(FP degrees)
        {
            return FPQuaternion.AngleAxis(degrees, FPVector3.Up);
        }

        public static FPVector3 Forward(FP yawDegrees)
        {
            FP radians = yawDegrees * FP.Deg2Rad;
            return new FPVector3(FP.Sin(radians), FP.Zero, FP.Cos(radians));
        }
    }

    public struct SportsArenaInput
    {
        public CYGPlayerInput Value;

        public bool Jump => Value.Jump;
        public bool Punch => Value.Action1;
        public bool Dash => Value.Action2;
        public bool Throw => Value.HasButton(CYGPlayerInput.ButtonAction3);
        public bool Block => Value.HasButton(CYGPlayerInput.ButtonAction4);

        public FPVector3 Move
        {
            get
            {
                FP x = FP.FromFraction(Value.MoveX, CYGPlayerInput.MaxMove);
                FP z = FP.FromFraction(Value.MoveY, CYGPlayerInput.MaxMove);
                return new FPVector3(x, FP.Zero, z);
            }
        }
    }

    public struct SportsArenaGoal
    {
        public FPAABB Area;
        public int DefendingTeam;
    }
}
