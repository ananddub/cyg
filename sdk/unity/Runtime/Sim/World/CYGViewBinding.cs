using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Deterministic entity-to-view binding. The simulation owns the prefab
    /// identity instead of the Unity presentation layer, so every client can
    /// spawn the same view for an entity created by a system or scene bake.
    ///
    /// The prefab is an <see cref="CYGAssetId"/> resolved through the view
    /// updater's prefab registry. Authoring code can still provide a direct
    /// prefab or string key as a presentation-only fallback.
    /// </summary>
    public struct CYGViewBinding : ICYGComponent
    {
        public const int TypeId = -1030;
        public const uint SchemaHashValue = 0x9C4E2A11u;
        public const int SizeInBytes = 6;

        /// <summary>Create the view from the verified/current simulation state.</summary>
        public const byte BindVerified = 0;

        /// <summary>Create the view from a predicted state when the updater supports it.</summary>
        public const byte BindPredicted = 1;

        /// <summary>Recycle the instance through the view pool.</summary>
        public const byte FlagPool = 1 << 0;

        /// <summary>The view is visible; systems can clear this for hidden entities.</summary>
        public const byte FlagVisible = 1 << 1;

        public CYGAssetId Prefab;
        public byte CreateBehaviour;
        public byte Flags;

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
            Prefab.WriteTo(buffer, offset);
            CYGWire.WriteByte(buffer, offset + 4, CreateBehaviour);
            CYGWire.WriteByte(buffer, offset + 5, Flags);
            return SizeInBytes;
        }

        public void ReadFrom(byte[] buffer, int offset)
        {
            CYGWire.RequireReadable(buffer, offset, SizeInBytes);
            Prefab.ReadFrom(buffer, offset);
            CreateBehaviour = CYGWire.ReadByte(buffer, offset + 4);
            Flags = CYGWire.ReadByte(buffer, offset + 5);
        }

        public void ComputeChecksum(ref uint crc)
        {
            Prefab.ComputeChecksum(ref crc);
            CYGChecksum.HashByte(ref crc, CreateBehaviour);
            CYGChecksum.HashByte(ref crc, Flags);
        }

        public static CYGViewBinding Create(
            CYGAssetId prefab,
            byte createBehaviour = BindVerified,
            byte flags = FlagPool | FlagVisible)
        {
            return new CYGViewBinding
            {
                Prefab = prefab,
                CreateBehaviour = createBehaviour,
                Flags = flags,
            };
        }
    }
}
