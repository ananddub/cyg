using System;
using System.Runtime.InteropServices;
using UnityEngine;

namespace OpenOxide
{
    /// <summary>
    /// Packed 20-byte player input frame streamed to authoritative OpenOxide server.
    /// </summary>
    [StructLayout(LayoutKind.Sequential, Pack = 1, Size = 20)]
    public struct PlayerInput
    {
        public uint Flags;      // Bit 16: Fire, Bit 32: Jump, Bit 64: Crouch
        public float AimYaw;    // Horizontal camera look angle in radians
        public float AimPitch;  // Vertical pitch angle in radians
        public float MoveX;     // Normalized horizontal strafe direction
        public float MoveZ;     // Normalized forward/backward direction

        public byte[] ToBytes()
        {
            byte[] bytes = new byte[20];
            Buffer.BlockCopy(BitConverter.GetBytes(Flags), 0, bytes, 0, 4);
            Buffer.BlockCopy(BitConverter.GetBytes(AimYaw), 0, bytes, 4, 4);
            Buffer.BlockCopy(BitConverter.GetBytes(AimPitch), 0, bytes, 8, 4);
            Buffer.BlockCopy(BitConverter.GetBytes(MoveX), 0, bytes, 12, 4);
            Buffer.BlockCopy(BitConverter.GetBytes(MoveZ), 0, bytes, 16, 4);
            return bytes;
        }

        public void WriteTo(byte[] buffer, int offset)
        {
            if (buffer == null || offset + 20 > buffer.Length) return;
            Buffer.BlockCopy(BitConverter.GetBytes(Flags), 0, buffer, offset, 4);
            Buffer.BlockCopy(BitConverter.GetBytes(AimYaw), 0, buffer, offset + 4, 4);
            Buffer.BlockCopy(BitConverter.GetBytes(AimPitch), 0, buffer, offset + 8, 4);
            Buffer.BlockCopy(BitConverter.GetBytes(MoveX), 0, buffer, offset + 12, 4);
            Buffer.BlockCopy(BitConverter.GetBytes(MoveZ), 0, buffer, offset + 16, 4);
        }
    }

    /// <summary>
    /// State of a player or tactical AI bot in the arena.
    /// </summary>
    public struct EntityState
    {
        public long Id;
        public Vector3 Position;
        public float Yaw;
        public float Pitch;
        public float Health;
        public float Score;
        public uint StateFlags;

        public bool IsBot => Id < 0;
        public bool IsGrounded => (StateFlags & 1) != 0;
        public bool IsCrouched => (StateFlags & 2) != 0;
        public bool IsFiring => (StateFlags & 4) != 0;
        public bool IsMoving => (StateFlags & 8) != 0;
    }

    /// <summary>
    /// Active 3D projectile in flight.
    /// </summary>
    public struct BulletState
    {
        public Vector3 Position;
        public long OwnerId;
    }

    /// <summary>
    /// Full unpacked authoritative state delta from 60Hz server tick.
    /// </summary>
    public class StateDelta
    {
        public long LocalPlayerId;
        public EntityState[] Entities;
        public BulletState[] Bullets;

        public static StateDelta Unpack(byte[] bytes, int startOffset = 0)
        {
            if (bytes == null || bytes.Length - startOffset < 12) return null;

            int offset = startOffset;
            long localId = BitConverter.ToInt64(bytes, offset);
            offset += 8;

            uint playerCount = BitConverter.ToUInt32(bytes, offset);
            offset += 4;

            // Bounds check to protect memory allocator
            if (playerCount > 256 || offset + (playerCount * 72) > bytes.Length) return null;

            EntityState[] entities = new EntityState[playerCount];
            for (int i = 0; i < playerCount; i++)
            {
                entities[i].Id = BitConverter.ToInt64(bytes, offset); offset += 8;
                double x = BitConverter.ToDouble(bytes, offset); offset += 8;
                double y = BitConverter.ToDouble(bytes, offset); offset += 8;
                double z = BitConverter.ToDouble(bytes, offset); offset += 8;
                entities[i].Position = new Vector3((float)x, (float)y, (float)z);

                entities[i].Yaw = (float)BitConverter.ToDouble(bytes, offset); offset += 8;
                entities[i].Pitch = (float)BitConverter.ToDouble(bytes, offset); offset += 8;
                entities[i].Health = (float)BitConverter.ToDouble(bytes, offset); offset += 8;
                entities[i].Score = (float)BitConverter.ToDouble(bytes, offset); offset += 8;
                entities[i].StateFlags = BitConverter.ToUInt32(bytes, offset); offset += 4;
                offset += 4; // padding
            }

            BulletState[] bullets = Array.Empty<BulletState>();
            if (offset + 4 <= bytes.Length)
            {
                uint bulletCount = BitConverter.ToUInt32(bytes, offset);
                offset += 4;

                if (bulletCount <= 1024 && offset + (bulletCount * 32) <= bytes.Length)
                {
                    bullets = new BulletState[bulletCount];
                    for (int i = 0; i < bulletCount; i++)
                    {
                        double bx = BitConverter.ToDouble(bytes, offset); offset += 8;
                        double by = BitConverter.ToDouble(bytes, offset); offset += 8;
                        double bz = BitConverter.ToDouble(bytes, offset); offset += 8;
                        bullets[i].Position = new Vector3((float)bx, (float)by, (float)bz);
                        bullets[i].OwnerId = BitConverter.ToInt64(bytes, offset); offset += 8;
                    }
                }
            }

            return new StateDelta
            {
                LocalPlayerId = localId,
                Entities = entities,
                Bullets = bullets
            };
        }
    }
}
