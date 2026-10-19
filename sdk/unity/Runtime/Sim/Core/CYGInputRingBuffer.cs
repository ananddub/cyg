using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Fixed-size circular input ring buffer for deterministic rollback netcode.
    /// Manages inputs per player across ticks with zero GC allocations during gameplay.
    /// Supports past input lookups, prediction of missing inputs (repeating last known input),
    /// and tracking confirmed vs predicted status.
    /// </summary>
    public sealed class CYGInputRingBuffer
    {
        public const int DefaultCapacity = 128; // Standard 128-tick history window (~2 seconds @ 60Hz)
        public const int DefaultMaxPlayers = 16;

        public struct Slot
        {
            public CYGPlayerInput Input;
            public bool HasInput;
            public bool IsPredicted;
        }

        private readonly int _capacity;
        private readonly int _mask;
        private readonly int _maxPlayers;

        // Flat 1D array for optimal cache locality: [playerIndex * _capacity + (tick & _mask)]
        private readonly Slot[] _slots;

        // Player ID tracking
        private readonly int[] _playerIds;
        private readonly bool[] _playerActive;
        private readonly int[] _latestReceivedTick;
        private readonly int[] _firstConfirmedTick;
        private readonly int[] _contiguousConfirmedTick;
        private readonly CYGPlayerInput[] _lastKnownInput;
        private readonly bool[] _hasLastKnownInput;

        private int _activePlayerCount;

        public int Capacity => _capacity;
        public int MaxPlayers => _maxPlayers;
        public int ActivePlayerCount => _activePlayerCount;

        public CYGInputRingBuffer(int capacity = DefaultCapacity, int maxPlayers = DefaultMaxPlayers)
        {
            // Capacity must be power of two for bitwise mask wrapping
            if (capacity < 16 || (capacity & (capacity - 1)) != 0)
                throw new ArgumentException("Capacity must be a power of two (e.g. 64, 128, 256)", nameof(capacity));

            _capacity = capacity;
            _mask = capacity - 1;
            _maxPlayers = maxPlayers;

            _slots = new Slot[_maxPlayers * _capacity];
            _playerIds = new int[_maxPlayers];
            _playerActive = new bool[_maxPlayers];
            _latestReceivedTick = new int[_maxPlayers];
            _firstConfirmedTick = new int[_maxPlayers];
            _contiguousConfirmedTick = new int[_maxPlayers];
            _lastKnownInput = new CYGPlayerInput[_maxPlayers];
            _hasLastKnownInput = new bool[_maxPlayers];

            Clear();
        }

        /// <summary>
        /// Registers or gets internal index for a playerId (0 to MaxPlayers-1). Zero GC.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public int GetOrAddPlayerIndex(int playerId)
        {
            // First check if already registered
            for (int i = 0; i < _activePlayerCount; i++)
            {
                if (_playerActive[i] && _playerIds[i] == playerId)
                    return i;
            }

            // Register new player slot
            if (_activePlayerCount < _maxPlayers)
            {
                int index = _activePlayerCount++;
                _playerIds[index] = playerId;
                _playerActive[index] = true;
                _latestReceivedTick[index] = -1;
                _firstConfirmedTick[index] = -1;
                _contiguousConfirmedTick[index] = -1;
                _hasLastKnownInput[index] = false;
                return index;
            }

            // Fallback: search for inactive slot
            for (int i = 0; i < _maxPlayers; i++)
            {
                if (!_playerActive[i])
                {
                    _playerIds[i] = playerId;
                    _playerActive[i] = true;
                    _latestReceivedTick[i] = -1;
                    _firstConfirmedTick[i] = -1;
                    _contiguousConfirmedTick[i] = -1;
                    _hasLastKnownInput[i] = false;
                    return i;
                }
            }

            throw new InvalidOperationException($"Exceeded maximum players limit ({_maxPlayers}) in CYGInputRingBuffer");
        }

        /// <summary>
        /// Stores an input for a specific player and tick.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void SetInput(int playerId, int tick, in CYGPlayerInput input, bool isPredicted = false)
        {
            int playerIndex = GetOrAddPlayerIndex(playerId);
            int slotIndex = (playerIndex * _capacity) + (tick & _mask);
            CYGPlayerInput stored = input.Clamped();
            stored.Tick = tick;

            _slots[slotIndex].Input = stored;
            _slots[slotIndex].HasInput = true;
            _slots[slotIndex].IsPredicted = isPredicted;

            if (!isPredicted)
            {
                _lastKnownInput[playerIndex] = stored;
                _hasLastKnownInput[playerIndex] = true;
                if (tick > _latestReceivedTick[playerIndex])
                {
                    _latestReceivedTick[playerIndex] = tick;
                }

                // Confirmed-tick tracking is contiguous, not "highest seen".
                // A gap (10, 12, 13) must keep the confirmed tick at 10 until
                // 11 actually arrives, otherwise rollback silently predicts
                // over packet loss forever.
                // The first confirmed tick only moves forward. A late packet
                // from before the first observed tick must not drag a healthy
                // 100-tick stream back to tick 50 (and force a huge rollback).
                if (_firstConfirmedTick[playerIndex] < 0)
                {
                    _firstConfirmedTick[playerIndex] = tick;
                }
                RecomputeContiguous(playerIndex);
            }
        }

        private void RecomputeContiguous(int playerIndex)
        {
            int first = _firstConfirmedTick[playerIndex];
            if (first < 0)
            {
                _contiguousConfirmedTick[playerIndex] = -1;
                return;
            }

            // The ring can only prove contiguity inside its own window. A
            // future/hostile tick must not turn this into a 2-billion-step
            // scan.
            int last = _latestReceivedTick[playerIndex];
            int windowEnd = first + _capacity - 1;
            if (last > windowEnd)
            {
                last = windowEnd;
            }

            int contiguous = first;
            while (contiguous < last)
            {
                int next = contiguous + 1;
                int nextSlot = (playerIndex * _capacity) + (next & _mask);
                ref Slot slot = ref _slots[nextSlot];
                if (!slot.HasInput || slot.IsPredicted || slot.Input.Tick != next)
                {
                    break;
                }
                contiguous = next;
            }
            _contiguousConfirmedTick[playerIndex] = contiguous;
        }

        /// <summary>
        /// Retrieves input for a player at a tick. If missing, predicts input by repeating last known input.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public CYGPlayerInput GetInput(int playerId, int tick)
        {
            int playerIndex = GetOrAddPlayerIndex(playerId);
            int slotIndex = (playerIndex * _capacity) + (tick & _mask);

            ref Slot slot = ref _slots[slotIndex];

            // If we have an exact match for this tick
            if (slot.HasInput && slot.Input.Tick == tick)
            {
                return slot.Input;
            }

            // Predict missing input (repeat last known input)
            CYGPlayerInput predicted;
            if (_hasLastKnownInput[playerIndex])
            {
                predicted = _lastKnownInput[playerIndex];
                predicted.Tick = tick;
            }
            else
            {
                predicted = new CYGPlayerInput(tick, 0, 0, 0);
            }

            // Cache prediction in slot
            slot.Input = predicted;
            slot.HasInput = true;
            slot.IsPredicted = true;

            return predicted;
        }

        /// <summary>
        /// Attempts to get existing input without generating prediction.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool TryGetInput(int playerId, int tick, out CYGPlayerInput input, out bool isPredicted)
        {
            int playerIndex = GetOrAddPlayerIndex(playerId);
            int slotIndex = (playerIndex * _capacity) + (tick & _mask);

            ref Slot slot = ref _slots[slotIndex];
            if (slot.HasInput && slot.Input.Tick == tick)
            {
                input = slot.Input;
                isPredicted = slot.IsPredicted;
                return true;
            }

            input = default;
            isPredicted = false;
            return false;
        }

        /// <summary>
        /// Checks if an input is confirmed (received from remote or local hardware, NOT predicted).
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool IsInputConfirmed(int playerId, int tick)
        {
            int playerIndex = GetOrAddPlayerIndex(playerId);
            int slotIndex = (playerIndex * _capacity) + (tick & _mask);
            ref Slot slot = ref _slots[slotIndex];
            return slot.HasInput && slot.Input.Tick == tick && !slot.IsPredicted;
        }

        /// <summary>
        /// Checks if an input exists and is marked as predicted.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public bool IsInputPredicted(int playerId, int tick)
        {
            int playerIndex = GetOrAddPlayerIndex(playerId);
            int slotIndex = (playerIndex * _capacity) + (tick & _mask);
            ref Slot slot = ref _slots[slotIndex];
            return slot.HasInput && slot.Input.Tick == tick && slot.IsPredicted;
        }

        /// <summary>
        /// Returns the highest tick index confirmed for the given player (-1 if none received).
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public int GetLatestReceivedTick(int playerId)
        {
            for (int i = 0; i < _activePlayerCount; i++)
            {
                if (_playerActive[i] && _playerIds[i] == playerId)
                    return _latestReceivedTick[i];
            }
            return -1;
        }

        /// <summary>
        /// Returns the lowest confirmed tick across all currently active players.
        /// Useful for advancing ConfirmedTick in rollback runner.
        /// </summary>
        public int GetLowestConfirmedTick(int fallbackTick)
        {
            if (_activePlayerCount == 0) return fallbackTick;

            // Players that joined later have no inputs before their first tick.
            // Ignore those earlier ticks rather than holding the whole room at
            // tick 0 forever. For players that were present, every tick through
            // their contiguous confirmed tick must be present.
            int latestFirst = -1;
            for (int i = 0; i < _activePlayerCount; i++)
            {
                if (_playerActive[i] && _firstConfirmedTick[i] > latestFirst)
                {
                    latestFirst = _firstConfirmedTick[i];
                }
            }

            if (latestFirst < 0)
            {
                return fallbackTick;
            }

            int lowest = int.MaxValue;
            for (int i = 0; i < _activePlayerCount; i++)
            {
                if (!_playerActive[i] || _firstConfirmedTick[i] < 0)
                {
                    continue;
                }
                if (_firstConfirmedTick[i] > latestFirst)
                {
                    continue; // not participating yet
                }
                int tick = _contiguousConfirmedTick[i];
                if (tick >= 0 && tick < lowest)
                {
                    lowest = tick;
                }
            }

            return lowest == int.MaxValue ? fallbackTick : lowest;
        }

        /// <summary>
        /// Fills outInputs array with inputs for all active players at the given tick. Zero GC.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void GetAllInputs(int tick, CYGPlayerInput[] outInputs, out int count)
        {
            count = _activePlayerCount;
            for (int i = 0; i < _activePlayerCount; i++)
            {
                outInputs[i] = GetInput(_playerIds[i], tick);
            }
        }

        /// <summary>
        /// Fills outInputs/outPlayerIds in matching slots. The simulation must
        /// map by player id, never by registration index: actor ids are sparse
        /// and arrival order is not entity order.
        /// </summary>
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public void GetAllInputs(
            int tick,
            CYGPlayerInput[] outInputs,
            int[] outPlayerIds,
            out int count)
        {
            count = _activePlayerCount;
            for (int i = 0; i < _activePlayerCount; i++)
            {
                int playerId = _playerIds[i];
                outPlayerIds[i] = playerId;
                outInputs[i] = GetInput(playerId, tick);
            }
        }

        /// <summary>
        /// Clears all slots and resets buffer state.
        /// </summary>
        public void Clear()
        {
            Array.Clear(_slots, 0, _slots.Length);
            Array.Clear(_playerIds, 0, _playerIds.Length);
            Array.Clear(_playerActive, 0, _playerActive.Length);
            Array.Clear(_latestReceivedTick, 0, _latestReceivedTick.Length);
            Array.Clear(_firstConfirmedTick, 0, _firstConfirmedTick.Length);
            Array.Clear(_contiguousConfirmedTick, 0, _contiguousConfirmedTick.Length);
            Array.Clear(_lastKnownInput, 0, _lastKnownInput.Length);
            Array.Clear(_hasLastKnownInput, 0, _hasLastKnownInput.Length);
            _activePlayerCount = 0;
        }
    }
}
