using System;
using System.Text;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Captures Unity player input, normalizes it into a deterministic CYGPlayerInput struct,
    /// injects it into the local CYGRollbackRunner, and transmits it over UDP to OpenOxide
    /// server using opcode 19 (CYG_WIRE_INPUT_TICK) with triple redundancy.
    /// Zero GC allocations during runtime tick loops.
    /// </summary>
    [DisallowMultipleComponent]
    [AddComponentMenu("CookYourGame/Sim/CYG Input Collector")]
    public class CYGInputCollector : MonoBehaviour
    {
        [Header("Input Configuration")]
        [Tooltip("Custom local player actor ID if not connected to server (default 1).")]
        [SerializeField] private int localActorId = 1;

        [Tooltip("Send redundant inputs (current + up to 2 previous inputs) to tolerate UDP packet loss.")]
        [SerializeField] private bool enableRedundancy = true;

        [Tooltip("Transmit inputs to network via CYGNetwork.SendRaw.")]
        [SerializeField] private bool sendToNetwork = true;

        [Header("Key Bindings")]
        [SerializeField] private KeyCode jumpKey = KeyCode.Space;
        [SerializeField] private KeyCode action1Key = KeyCode.Mouse0;
        [SerializeField] private KeyCode action1KeyAlt = KeyCode.J;
        [SerializeField] private KeyCode action2Key = KeyCode.LeftShift;
        [SerializeField] private KeyCode action2KeyAlt = KeyCode.K;

        // Redundancy tracking (last 2 inputs)
        private CYGPlayerInput _prevInput1;
        private CYGPlayerInput _prevInput2;
        private bool _hasPrev1;
        private bool _hasPrev2;

        // Reusable zero-allocation packet buffer:
        // 16 bytes header + up to 27 bytes payload (3 x 9B) = 43 bytes max
        private readonly byte[] _sendBuffer = new byte[64];
        private byte[] _cachedRoomIdBytes = new byte[8];
        private string _lastRoomName;
        private bool _cachedRoomIdValid;

        #region Properties

        public int LocalActorId
        {
            get => CYGNetwork.LocalActorNumber > 0 ? CYGNetwork.LocalActorNumber : localActorId;
            set => localActorId = value;
        }

        public bool SendToNetwork
        {
            get => sendToNetwork;
            set => sendToNetwork = value;
        }

        #endregion

        private void Awake()
        {
            UpdateRoomIdCache();
        }

        private void UpdateRoomIdCache()
        {
            string roomName = CYGNetwork.RoomName;
            bool valid = !string.IsNullOrEmpty(roomName);
            if (roomName != _lastRoomName)
            {
                _lastRoomName = roomName;
                _cachedRoomIdValid = valid;
                Array.Clear(_cachedRoomIdBytes, 0, 8);
                if (valid)
                {
                    byte[] raw = Encoding.UTF8.GetBytes(roomName);
                    int copyLen = Math.Min(raw.Length, 8);
                    Buffer.BlockCopy(raw, 0, _cachedRoomIdBytes, 0, copyLen);
                }
            }
            else
            {
                _cachedRoomIdValid = valid;
            }
        }

        /// <summary>
        /// Polls Unity input hardware and returns a packed CYGPlayerInput struct for the given tick.
        /// Zero GC allocations.
        /// </summary>
        public CYGPlayerInput PollInput(int tick)
        {
            float h = 0f;
            float v = 0f;

            // 1. Try Unity Input Manager axes
            try
            {
                h = Input.GetAxisRaw("Horizontal");
                v = Input.GetAxisRaw("Vertical");
            }
            catch
            {
                // If Unity project has Input Manager disabled, fallback gracefully to key polling
            }

            // 2. Direct key poll fallback / supplement (WASD + Arrow keys)
            if (Input.GetKey(KeyCode.W) || Input.GetKey(KeyCode.UpArrow)) v += 1f;
            if (Input.GetKey(KeyCode.S) || Input.GetKey(KeyCode.DownArrow)) v -= 1f;
            if (Input.GetKey(KeyCode.D) || Input.GetKey(KeyCode.RightArrow)) h += 1f;
            if (Input.GetKey(KeyCode.A) || Input.GetKey(KeyCode.LeftArrow)) h -= 1f;

            // Clamp normalized axes to [-1.0, +1.0]
            h = Mathf.Clamp(h, -1f, 1f);
            v = Mathf.Clamp(v, -1f, 1f);

            // Convert to short fixed range [-1000, +1000]
            short moveX = (short)Mathf.Clamp((int)(h * 1000f), -1000, 1000);
            short moveY = (short)Mathf.Clamp((int)(v * 1000f), -1000, 1000);

            // 3. Buttons bitmask
            byte buttons = 0;

            // Jump (Bit 0)
            bool jump = false;
            try { jump = Input.GetButton("Jump"); } catch { }
            if (jump || Input.GetKey(jumpKey))
            {
                buttons |= CYGPlayerInput.ButtonJump;
            }

            // Action1 (Bit 1)
            bool action1 = false;
            try { action1 = Input.GetButton("Fire1"); } catch { }
            if (action1 || Input.GetKey(action1Key) || Input.GetKey(action1KeyAlt))
            {
                buttons |= CYGPlayerInput.ButtonAction1;
            }

            // Action2 (Bit 2)
            bool action2 = false;
            try { action2 = Input.GetButton("Fire2"); } catch { }
            if (action2 || Input.GetKey(action2Key) || Input.GetKey(action2KeyAlt))
            {
                buttons |= CYGPlayerInput.ButtonAction2;
            }

            return new CYGPlayerInput(tick, buttons, moveX, moveY);
        }

        /// <summary>
        /// Polls current input, injects into local runner, packages redundant UDP packet,
        /// and sends over CYGNetwork.
        /// </summary>
        public CYGPlayerInput PollAndDispatch(int tick, CYGRollbackRunner runner)
        {
            CYGPlayerInput input = PollInput(tick);

            // 1. Hand input to local rollback runner
            if (runner != null)
            {
                runner.SetLocalInput(tick, in input);
            }

            // 2. Transmit to network if enabled
            if (sendToNetwork && CYGNetwork.IsConnected)
            {
                SendInputPacket(input, tick);
            }

            // 3. Shift redundancy history
            if (enableRedundancy)
            {
                _prevInput2 = _prevInput1;
                _hasPrev2 = _hasPrev1;

                _prevInput1 = input;
                _hasPrev1 = true;
            }

            return input;
        }

        /// <summary>
        /// Serializes and sends CYG_WIRE_INPUT_TICK packet over UDP. Zero GC allocations.
        /// Wire Framing:
        /// [0]      : opcode (19 = CYG_WIRE_INPUT_TICK)
        /// [1..8]   : room_id (8 bytes)
        /// [9..12]  : client_tick (u32 BE)
        /// [13..14] : player_actor_id (u16 BE)
        /// [15]     : input_len (u8)
        /// [16..]   : input_payload (current 9B + optional prev1 9B + prev2 9B)
        /// </summary>
        private void SendInputPacket(in CYGPlayerInput currentInput, int tick)
        {
            UpdateRoomIdCache();
            if (!_cachedRoomIdValid)
            {
                // Not joined yet: sending an all-zero room id would be rejected
                // by the server (and must never be treated as a wildcard).
                return;
            }

            // Byte 0: Opcode 19
            _sendBuffer[0] = CYGNetwork.CYG_WIRE_INPUT_TICK;

            // Bytes 1..8: room_id
            Buffer.BlockCopy(_cachedRoomIdBytes, 0, _sendBuffer, 1, 8);

            // Bytes 9..12: client_tick (big-endian u32)
            _sendBuffer[9] = (byte)((tick >> 24) & 0xFF);
            _sendBuffer[10] = (byte)((tick >> 16) & 0xFF);
            _sendBuffer[11] = (byte)((tick >> 8) & 0xFF);
            _sendBuffer[12] = (byte)(tick & 0xFF);

            // Bytes 13..14: player_actor_id (big-endian u16)
            ushort actor = (ushort)LocalActorId;
            _sendBuffer[13] = (byte)((actor >> 8) & 0xFF);
            _sendBuffer[14] = (byte)(actor & 0xFF);

            // Bytes 16..: input_payload
            int payloadOffset = 16;

            // Write current tick input (9 bytes)
            currentInput.WriteTo(_sendBuffer, payloadOffset);
            payloadOffset += CYGPlayerInput.SizeInBytes;

            // Redundancy: append last 2 previous inputs
            if (enableRedundancy)
            {
                if (_hasPrev1)
                {
                    _prevInput1.WriteTo(_sendBuffer, payloadOffset);
                    payloadOffset += CYGPlayerInput.SizeInBytes;

                    if (_hasPrev2)
                    {
                        _prevInput2.WriteTo(_sendBuffer, payloadOffset);
                        payloadOffset += CYGPlayerInput.SizeInBytes;
                    }
                }
            }

            int payloadLength = payloadOffset - 16;
            _sendBuffer[15] = (byte)payloadLength;

            int totalPacketSize = payloadOffset;

            // Transmit via CYGNetwork.SendRaw directly onto the non-blocking UDP transport
            CYGNetwork.SendRaw(CYGNetwork.CYG_WIRE_INPUT_TICK, new ArraySegment<byte>(_sendBuffer, 0, totalPacketSize));
        }

        /// <summary>
        /// Clears previous input history (e.g. on match start or disconnect).
        /// </summary>
        public void ResetHistory()
        {
            _hasPrev1 = false;
            _hasPrev2 = false;
            _prevInput1 = default;
            _prevInput2 = default;
        }
    }
}
