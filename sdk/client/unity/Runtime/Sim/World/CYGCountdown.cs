using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// ECS wrapper around <see cref="CYGCountdownTimer"/> so a countdown is
    /// owned by the simulation and included in snapshots, rollback and the
    /// world checksum. Add <see cref="CYGCountdownSystem"/> to tick every
    /// countdown automatically.
    /// </summary>
    public struct CYGCountdown : ICYGComponent
    {
        public const int TypeId = -1031;
        public const uint SchemaHashValue = 0x6D1B9F03u;
        public const int SizeInBytes = 17;

        public CYGCountdownTimer Timer;

        public int ComponentTypeId
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return TypeId; }
        }

        public uint ComponentSchemaHash
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return SchemaHashValue; }
        }

        public int WireSize
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return SizeInBytes; }
        }

        public int WriteTo(byte[] buffer, int offset)
        {
            CYGWire.RequireWritable(buffer, offset, SizeInBytes);
            Timer.Duration.WriteTo(buffer, offset);
            Timer.Remaining.WriteTo(buffer, offset + 8);
            CYGWire.WriteByte(buffer, offset + 16, Timer.Flags);
            return SizeInBytes;
        }

        public void ReadFrom(byte[] buffer, int offset)
        {
            CYGWire.RequireReadable(buffer, offset, SizeInBytes);
            Timer.Duration.ReadFrom(buffer, offset);
            Timer.Remaining.ReadFrom(buffer, offset + 8);
            Timer.Flags = CYGWire.ReadByte(buffer, offset + 16);
        }

        public void ComputeChecksum(ref uint crc)
        {
            Timer.Duration.ComputeChecksum(ref crc);
            Timer.Remaining.ComputeChecksum(ref crc);
            CYGChecksum.HashByte(ref crc, Timer.Flags);
        }

        public static CYGCountdown Create(FP duration, bool loop = false, bool start = true)
        {
            return new CYGCountdown
            {
                Timer = CYGCountdownTimer.Create(duration, loop, start),
            };
        }
    }

    /// <summary>
    /// Advances every <see cref="CYGCountdown"/> in the world by the fixed
    /// delta time. Expiry edges are raised through <see cref="OnExpired"/> so
    /// gameplay code can apply the effect (respawn, ability charge, ...).
    /// </summary>
    public sealed class CYGCountdownSystem : CYGSystem
    {
        /// <summary>Raised once on the tick a countdown reaches zero.</summary>
        public event Action<CYGWorld, CYGEntityRef> OnExpired;

        public override void OnInit(CYGWorld world)
        {
            if (!world.TryGetStore(CYGCountdown.TypeId, out _))
            {
                world.Register<CYGCountdown>();
            }
        }

        public override void OnUpdate(in CYGSystemContext context)
        {
            CYGWorld world = context.World;
            FP deltaTime = context.DeltaTime;

            for (int i = 0; i < world.SlotCount; i++)
            {
                CYGEntityRef entity;
                if (!world.TryGetEntityAt(i, out entity))
                {
                    continue;
                }

                CYGCountdown countdown;
                if (!world.TryGet<CYGCountdown>(entity, out countdown))
                {
                    continue;
                }

                if (countdown.Timer.Tick(deltaTime))
                {
                    world.Set(entity, countdown);
                    OnExpired?.Invoke(world, entity);
                }
                else if (countdown.Timer.IsRunning)
                {
                    world.Set(entity, countdown);
                }
            }
        }
    }
}
