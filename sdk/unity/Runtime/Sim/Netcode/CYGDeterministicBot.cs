using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Deterministic input generator for in-editor and headless matches. The
    /// result is a pure function of player id and tick, so every client can
    /// rebuild the same stream without a transport.
    /// </summary>
    public static class CYGDeterministicBot
    {
        private const int SegmentTicks = 48;

        private static readonly short[] DirectionX = { 1000, 707, 0, -707, -1000, -707, 0, 707 };
        private static readonly short[] DirectionZ = { 0, 707, 1000, 707, 0, -707, -1000, -707 };

        public static CYGPlayerInput Sample(int playerId, int tick)
        {
            if (playerId <= 0)
            {
                throw new ArgumentOutOfRangeException(nameof(playerId));
            }
            if (tick < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(tick));
            }

            int segment = ((tick / SegmentTicks) + playerId * 3) & 7;
            short moveX = DirectionX[segment];
            short moveY = DirectionZ[segment];
            if ((tick + playerId * 7) % 137 < 6)
            {
                moveX = 0;
                moveY = 0;
            }

            byte buttons = 0;
            if ((tick + playerId * 11) % 97 == 0)
            {
                buttons |= CYGPlayerInput.ButtonJump;
            }
            if ((tick + playerId * 5) % 43 == 0)
            {
                buttons |= CYGPlayerInput.ButtonAction1;
            }
            return new CYGPlayerInput(tick, buttons, moveX, moveY);
        }
    }
}
