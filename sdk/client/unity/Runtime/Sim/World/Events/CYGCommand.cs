using System;

namespace CookYourGame.Sim
{
    [Flags]
    public enum CYGCommandFlags : byte
    {
        None = 0,

        /// <summary>The command is recorded but must not execute.</summary>
        Disabled = 1 << 0,

        /// <summary>The command stays active for following ticks until replaced.</summary>
        Repeat = 1 << 1,

        /// <summary>The local client may execute the command before verification.</summary>
        Predictable = 1 << 2,

        /// <summary>Skip the command while the world is being re-simulated.</summary>
        IgnoreOnResimulate = 1 << 3
    }

    public enum CYGCommandSendMode
    {
        Reliable,
        Unreliable
    }

    /// <summary>
    /// Player command executed by a simulation system. Commands are external
    /// input, so they are neither hashed nor captured in world snapshots; the
    /// command buffer keeps them independently of rollback.
    /// </summary>
    public abstract class CYGCommand
    {
        public const int MaxPayloadSize = 256;

        public abstract int TypeId { get; }

        public virtual CYGCommandFlags Flags
        {
            get { return CYGCommandFlags.Predictable; }
        }

        public virtual CYGCommandSendMode SendMode
        {
            get { return CYGCommandSendMode.Reliable; }
        }

        /// <summary>Player that issued the command; assigned by the command buffer.</summary>
        public int PlayerId { get; internal set; }

        /// <summary>Tick the command applies to; assigned by the command buffer.</summary>
        public int Tick { get; internal set; }

        /// <summary>Exact number of payload bytes written by <see cref="WritePayload"/>.</summary>
        public abstract int PayloadSize { get; }

        public abstract int WritePayload(byte[] buffer, int offset);

        public abstract void ReadPayload(byte[] buffer, int offset, int length);

        public bool IsDisabled
        {
            get { return (Flags & CYGCommandFlags.Disabled) != 0; }
        }

        public bool IsRepeat
        {
            get { return (Flags & CYGCommandFlags.Repeat) != 0; }
        }

        public bool IsPredictable
        {
            get { return (Flags & CYGCommandFlags.Predictable) != 0; }
        }

        public bool IsIgnoredOnResimulate
        {
            get { return (Flags & CYGCommandFlags.IgnoreOnResimulate) != 0; }
        }
    }
}
