using System;

namespace CookYourGame.Sim
{
    /// <summary>Input metadata that is independent of the wire payload.</summary>
    [Flags]
    public enum CYGInputFlags
    {
        None = 0,

        /// <summary>The input may be repeated when a later tick has no sample.</summary>
        Repeatable = 1 << 0,
    }

    /// <summary>
    /// Per-tick input poll object passed to <see cref="CYGCallbacks.InputPoll"/>.
    /// Mirrors reference's CallbackPollInput shape without exposing reference types:
    /// listeners can check <see cref="IsInputSet"/> and call
    /// <see cref="SetInput(in CYGPlayerInput, CYGInputFlags)"/>.
    /// The same instance is reused every tick, so do not store it.
    /// </summary>
    public sealed class CYGInputPoll
    {
        public CYGPlayerRef Player { get; private set; }
        public int Tick { get; private set; }
        public bool IsInputSet { get; private set; }
        public CYGPlayerInput Input { get; private set; }
        public CYGInputFlags Flags { get; private set; }

        internal void Reset(CYGPlayerRef player, int tick)
        {
            Player = player;
            Tick = tick;
            IsInputSet = false;
            Input = new CYGPlayerInput(tick, 0, 0, 0);
            Flags = CYGInputFlags.None;
        }

        /// <summary>Sets the input for this tick. Later listeners may override it.</summary>
        public void SetInput(in CYGPlayerInput input, CYGInputFlags flags = CYGInputFlags.None)
        {
            CYGPlayerInput value = input.Clamped();
            value.Tick = Tick;
            Input = value;
            Flags = flags;
            IsInputSet = true;
        }
    }
}
