using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Deterministic player input struct (9 bytes, packed sequential).
    /// Used for input capture, network broadcast, ring buffers, and rollback replay.
    /// </summary>
    [StructLayout(LayoutKind.Sequential, Pack = 1)]
    public struct CYGPlayerInput : IEquatable<CYGPlayerInput>
    {
        public const int SizeInBytes = 9;

        // Bit flags for Buttons byte
        public const byte ButtonJump = 1 << 0;     // Bit 0: Jump
        public const byte ButtonAction1 = 1 << 1;  // Bit 1: Action1 / Attack / Primary
        public const byte ButtonAction2 = 1 << 2;  // Bit 2: Action2 / Sprint / Secondary
        public const byte ButtonAction3 = 1 << 3;  // Bit 3: Extra action
        public const byte ButtonAction4 = 1 << 4;  // Bit 4: Extra action
        public const byte ButtonMask = 0x1F;
        public const short MaxMove = 1000;

        /// <summary>Simulation tick index for which this input was generated.</summary>
        public int Tick;

        /// <summary>Bitfield representing button actions (Bit 0: Jump, Bit 1: Action1, Bit 2: Action2).</summary>
        public byte Buttons;

        /// <summary>Horizontal movement axis (-1000 to +1000, where 1000 represents 1.0).</summary>
        public short MoveX;

        /// <summary>Vertical movement axis (-1000 to +1000, where 1000 represents 1.0).</summary>
        public short MoveY;

        public CYGPlayerInput(int tick, byte buttons, short moveX, short moveY)
        {
            Tick = tick;
            Buttons = (byte)(buttons & ButtonMask);
            MoveX = ClampMove(moveX);
            MoveY = ClampMove(moveY);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        private static short ClampMove(short value)
        {
            if (value > MaxMove) return MaxMove;
            if (value < -MaxMove) return (short)-MaxMove;
            return value;
        }

        /// <summary>
        /// Returns a copy with untrusted network/device values clamped to the
        /// documented range. Call this at every trust boundary (wire read,
        /// ring-buffer write, simulation step).
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public CYGPlayerInput Clamped()
        {
            return new CYGPlayerInput(
                Tick,
                (byte)(Buttons & ButtonMask),
                ClampMove(MoveX),
                ClampMove(MoveY));
        }

        #region Convenience Accessors

        public bool Jump
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => (Buttons & ButtonJump) != 0;
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            set => SetButton(ButtonJump, value);
        }

        public bool Action1
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => (Buttons & ButtonAction1) != 0;
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            set => SetButton(ButtonAction1, value);
        }

        public bool Action2
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => (Buttons & ButtonAction2) != 0;
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            set => SetButton(ButtonAction2, value);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool HasButton(byte flag) => (Buttons & flag) != 0;

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void SetButton(byte flag, bool pressed)
        {
            if (pressed)
                Buttons |= flag;
            else
                Buttons = (byte)(Buttons & ~flag);
            Buttons &= ButtonMask;
        }

        /// <summary>Normalized float representation of MoveX (-1.0f to 1.0f) for rendering/view.</summary>
        public float NormalizedMoveX
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => MoveX / 1000f;
        }

        /// <summary>Normalized float representation of MoveY (-1.0f to 1.0f) for rendering/view.</summary>
        public float NormalizedMoveY
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => MoveY / 1000f;
        }

        #endregion

        #region Serialization (Zero GC)

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void WriteTo(byte[] buffer, int offset)
        {
            buffer[offset] = (byte)(Tick & 0xFF);
            buffer[offset + 1] = (byte)((Tick >> 8) & 0xFF);
            buffer[offset + 2] = (byte)((Tick >> 16) & 0xFF);
            buffer[offset + 3] = (byte)((Tick >> 24) & 0xFF);
            buffer[offset + 4] = Buttons;
            buffer[offset + 5] = (byte)(MoveX & 0xFF);
            buffer[offset + 6] = (byte)((MoveX >> 8) & 0xFF);
            buffer[offset + 7] = (byte)(MoveY & 0xFF);
            buffer[offset + 8] = (byte)((MoveY >> 8) & 0xFF);
        }

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public static CYGPlayerInput ReadFrom(byte[] buffer, int offset)
        {
            int tick = buffer[offset]
                     | (buffer[offset + 1] << 8)
                     | (buffer[offset + 2] << 16)
                     | (buffer[offset + 3] << 24);
            byte buttons = buffer[offset + 4];
            short moveX = (short)(buffer[offset + 5] | (buffer[offset + 6] << 8));
            short moveY = (short)(buffer[offset + 7] | (buffer[offset + 8] << 8));
            return new CYGPlayerInput(tick, buttons, moveX, moveY);
        }

        #endregion

        #region Equality

        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool Equals(CYGPlayerInput other)
        {
            return Tick == other.Tick
                && Buttons == other.Buttons
                && MoveX == other.MoveX
                && MoveY == other.MoveY;
        }

        /// <summary>
        /// Compares input actions only (Buttons, MoveX, MoveY) ignoring Tick.
        /// Useful for checking whether inputs changed between ticks.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool ActionsEqual(in CYGPlayerInput other)
        {
            return Buttons == other.Buttons
                && MoveX == other.MoveX
                && MoveY == other.MoveY;
        }

        public override bool Equals(object obj) => obj is CYGPlayerInput other && Equals(other);

        public override int GetHashCode()
        {
            unchecked
            {
                int hash = Tick;
                hash = (hash * 397) ^ Buttons.GetHashCode();
                hash = (hash * 397) ^ MoveX.GetHashCode();
                hash = (hash * 397) ^ MoveY.GetHashCode();
                return hash;
            }
        }

        public static bool operator ==(CYGPlayerInput left, CYGPlayerInput right) => left.Equals(right);
        public static bool operator !=(CYGPlayerInput left, CYGPlayerInput right) => !left.Equals(right);

        public override string ToString()
        {
            return $"CYGPlayerInput(Tick: {Tick}, Btn: {Buttons}, MoveX: {MoveX}, MoveY: {MoveY})";
        }

        #endregion
    }
}
