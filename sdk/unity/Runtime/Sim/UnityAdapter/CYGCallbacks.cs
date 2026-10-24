using System;
using UnityEngine;

#if ENABLE_INPUT_SYSTEM
using UnityEngine.InputSystem;
#endif

namespace CookYourGame.Sim
{
    /// <summary>
    /// Typed gameplay callback surface, mirroring referenceCallbacks. The runner
    /// invokes <see cref="OnInputPolling"/> once per simulation tick; the
    /// override calls <see cref="SubmitInput"/> with the local input for that
    /// tick. Game start, simulation update, view update, late view update and
    /// rollback callbacks are invoked at their matching runner phases.
    /// </summary>
    [DisallowMultipleComponent]
    [AddComponentMenu("CookYourGame/Sim/CYG Callbacks")]
    public class CYGCallbacks : CYGMonoBehaviour
    {
        private bool _hasInput;
        private CYGPlayerInput _input;
        private CYGInputFlags _inputFlags;
        private int _inputTick;
        private readonly CYGInputPoll _poll = new CYGInputPoll();

        public bool HasInputThisTick => _hasInput;
        public CYGPlayerInput PolledInput => _input;
        public CYGInputFlags InputFlags => _inputFlags;
        public int InputTick => _inputTick;

        /// <summary>
        /// reference-style input poll event. Listeners can inspect
        /// <see cref="CYGInputPoll.IsInputSet"/> and call
        /// <see cref="CYGInputPoll.SetInput(in CYGPlayerInput, CYGInputFlags)"/>.
        /// The override-based <see cref="OnInputPolling"/> remains supported.
        /// </summary>
        public event Action<CYGInputPoll> InputPoll;

        public event Action<CYGPlayerRef, CYGPlayerInput> InputPolled;
        public event Action GameStarted;
        public event Action<int, int> RolledBack;

        protected virtual void Awake()
        {
            CYGDeterministicRunner runner;
            if (TryGetRunner(out runner))
            {
                runner.AttachCallbacks(this);
            }
        }

        /// <summary>Invoked when the runner builds simulation systems. Override to register user systems.</summary>
        protected virtual void OnInitSystems(CYGSystemRunner systems) { }

        /// <summary>Invoked when the runner polls deterministic input for a tick.</summary>
        protected virtual void OnInputPolling(CYGPlayerRef player) { }

        /// <summary>Invoked once when the simulation starts.</summary>
        protected virtual void OnGameStart() { }

        /// <summary>Invoked after a full state restore or resync.</summary>
        protected virtual void OnGameResync() { }

        /// <summary>Invoked immediately after deterministic ECS ticks advance in the frame.</summary>
        protected virtual void OnSimulationUpdate() { }

        /// <summary>Invoked during the visual frame update after view interpolation.</summary>
        protected virtual void OnViewUpdate() { }

        /// <summary>Invoked during LateUpdate for camera tracking and final presentation.</summary>
        protected virtual void OnLateViewUpdate() { }

        /// <summary>Invoked when a rollback occurs.</summary>
        protected virtual void OnRollback(int fromTick, int toTick) { }

        /// <summary>Invoked when a new visual entity view is created or spawned from the pool.</summary>
        protected virtual void OnViewSpawned(CYGEntityView view) { }

        /// <summary>Invoked when an entity view is returned to pool or destroyed.</summary>
        protected virtual void OnViewDespawned(CYGEntityView view) { }

        /// <summary>Invoked when an entity view is bound or re-bound to an entity/player.</summary>
        protected virtual void OnViewBound(CYGEntityView view) { }

        /// <summary>Submits the local input for the tick currently being polled.</summary>
        public void SubmitInput(in CYGPlayerInput input)
        {
            SubmitInput(input, CYGInputFlags.None);
        }

        /// <summary>Submits input and optional metadata for the current polled tick.</summary>
        public void SubmitInput(in CYGPlayerInput input, CYGInputFlags flags)
        {
            _input = input.Clamped();
            _inputFlags = flags;
            _hasInput = true;
        }

        /// <summary>reference-style alias for <see cref="SubmitInput"/>.</summary>
        public void SetPlayerInput(in CYGPlayerInput input)
        {
            SubmitInput(input);
        }

        /// <summary>
        /// Submits input for a specific player ID. This is local-simulation
        /// only: the input hub authenticates every packet to the sending peer,
        /// so it cannot transmit a second local actor over one connection.
        /// Useful for local multiplayer and bots.
        /// </summary>
        public void SetPlayerInput(int playerId, in CYGPlayerInput input)
        {
            CYGPlayerInput clamped = input.Clamped();
            int tick = _inputTick >= 0 ? _inputTick : CurrentTick;
            clamped.Tick = tick;
            if (Session != null)
            {
                Session.SetPlayerInput(playerId, tick, clamped, isPredicted: false);
                if (playerId == LocalPlayerId)
                {
                    // Prevent the runner from also falling back to hardware
                    // polling when the local player supplied input this tick.
                    _input = clamped;
                    _inputFlags = CYGInputFlags.None;
                    _hasInput = true;
                }
            }
        }

        /// <summary>Convenience helper for axis/button input polled from any device backend.</summary>
        public void SubmitMove(short moveX, short moveY, bool jump = false, bool action1 = false)
        {
            byte buttons = 0;
            if (jump)
            {
                buttons |= CYGPlayerInput.ButtonJump;
            }
            if (action1)
            {
                buttons |= CYGPlayerInput.ButtonAction1;
            }
            SubmitInput(new CYGPlayerInput(_inputTick, buttons, moveX, moveY));
        }

        #region Hardware Input Helpers

        /// <summary>Checks whether a key is pressed using New Input System or Legacy Input.</summary>
        public static bool IsKeyPressed(KeyCode key)
        {
#if ENABLE_INPUT_SYSTEM
            var kb = Keyboard.current;
            if (kb != null)
            {
                switch (key)
                {
                    case KeyCode.A: return kb.aKey.isPressed;
                    case KeyCode.D: return kb.dKey.isPressed;
                    case KeyCode.S: return kb.sKey.isPressed;
                    case KeyCode.W: return kb.wKey.isPressed;
                    case KeyCode.LeftArrow: return kb.leftArrowKey.isPressed;
                    case KeyCode.RightArrow: return kb.rightArrowKey.isPressed;
                    case KeyCode.DownArrow: return kb.downArrowKey.isPressed;
                    case KeyCode.UpArrow: return kb.upArrowKey.isPressed;
                    case KeyCode.Space: return kb.spaceKey.isPressed;
                    case KeyCode.Return: return kb.enterKey.isPressed;
                }
            }
#endif
            try
            {
                return UnityEngine.Input.GetKey(key);
            }
            catch
            {
                return false;
            }
        }

        /// <summary>Reads a keyboard axis cleanly (-max .. +max).</summary>
        public static short ReadAxis(KeyCode negative, KeyCode positive, short max = 1000)
        {
            short val = 0;
            if (IsKeyPressed(negative)) val -= max;
            if (IsKeyPressed(positive)) val += max;
            return val;
        }

        /// <summary>Reads a 2D movement vector with diagonal normalization.</summary>
        public static (short x, short y) ReadMoveVector(KeyCode left, KeyCode right, KeyCode down, KeyCode up)
        {
            short x = ReadAxis(left, right);
            short y = ReadAxis(down, up);
            if (x != 0 && y != 0)
            {
                x = (short)(x * 0.7071f);
                y = (short)(y * 0.7071f);
            }
            return (x, y);
        }

        /// <summary>Reads WASD movement vector.</summary>
        public static (short x, short y) ReadMoveVectorWASD() => ReadMoveVector(KeyCode.A, KeyCode.D, KeyCode.S, KeyCode.W);

        /// <summary>Reads Arrow keys movement vector.</summary>
        public static (short x, short y) ReadMoveVectorArrows() => ReadMoveVector(KeyCode.LeftArrow, KeyCode.RightArrow, KeyCode.DownArrow, KeyCode.UpArrow);

        #endregion

        internal void PollForTick(int tick, int localPlayerId)
        {
            _hasInput = false;
            _inputFlags = CYGInputFlags.None;
            _inputTick = tick;
            _input = new CYGPlayerInput(tick, 0, 0, 0);

            CYGPlayerRef player = localPlayerId > 0
                ? new CYGPlayerRef((ushort)localPlayerId)
                : CYGPlayerRef.None;

            _poll.Reset(player, tick);
            OnInputPolling(player);
            InputPoll?.Invoke(_poll);

            if (_poll.IsInputSet)
            {
                _input = _poll.Input;
                _inputFlags = _poll.Flags;
                _hasInput = true;
            }

            if (_hasInput)
            {
                _input.Tick = tick;
                InputPolled?.Invoke(player, _input);
            }
        }

        /// <summary>Raises the game-start callback. Invoked by the runner once simulation starts.</summary>
        public void NotifyGameStart()
        {
            OnGameStart();
            GameStarted?.Invoke();
        }

        public void NotifyGameResync() => OnGameResync();
        internal void NotifySimulationUpdate() => OnSimulationUpdate();
        internal void NotifyViewUpdate() => OnViewUpdate();
        internal void NotifyLateViewUpdate() => OnLateViewUpdate();

        internal void NotifyRollback(int fromTick, int toTick)
        {
            OnRollback(fromTick, toTick);
            RolledBack?.Invoke(fromTick, toTick);
        }

        internal void NotifyInitSystems(CYGSystemRunner systems) => OnInitSystems(systems);
        internal void NotifyViewSpawned(CYGEntityView view) => OnViewSpawned(view);
        internal void NotifyViewDespawned(CYGEntityView view) => OnViewDespawned(view);
        internal void NotifyViewBound(CYGEntityView view) => OnViewBound(view);
    }
}
