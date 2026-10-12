using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using UnityEngine;

namespace OpenOxide
{
    [Serializable]
    public class ReplayFrameData
    {
        public int t;
        public byte[] d;
    }

    [Serializable]
    public class MatchReplayData
    {
        public string format = "oxr-v1";
        public string room;
        public string recordedAt;
        public int durationMs;
        public int totalFrames;
        public List<ReplayFrameData> frames = new List<ReplayFrameData>();
    }

    /// <summary>
    /// Active OpenOxide match room connection.
    /// Handles packet framing, main-thread synchronization, input streaming, and match recording.
    /// </summary>
    public class OpenOxideRoom : IDisposable
    {
        public string RoomName { get; private set; }
        public bool IsSpectator { get; private set; }
        public long LocalPlayerId { get; private set; }

        public IOpenOxideSocket Socket => _socket;
        private IOpenOxideSocket _socket;
        private readonly ConcurrentQueue<byte[]> _inboundQueue = new ConcurrentQueue<byte[]>();

        // Replay Recording
        private bool _isRecording;
        private float _recordStartTime;
        private readonly List<ReplayFrameData> _recordedFrames = new List<ReplayFrameData>();

        public event Action<StateDelta> OnDelta;
        public event Action<byte[]> OnRawPacket;

        public OpenOxideRoom(string roomName, bool isSpectator = false)
        {
            RoomName = roomName;
            IsSpectator = isSpectator;
        }

        public void AttachSocket(IOpenOxideSocket socket)
        {
            _socket = socket;
            _socket.OnPacketReceived += HandleInboundPacket;

            if (IsSpectator)
            {
                // Spectator handshake protocol
                byte[] specHandshake = new byte[] { 0xFF, 0x53, 0x50, 0x45, 0x43 };
                _socket.Send(specHandshake);
            }
        }

        private void HandleInboundPacket(byte[] data)
        {
            if (data == null || data.Length == 0) return;
            _inboundQueue.Enqueue(data);
        }

        /// <summary>
        /// Must be called every Unity frame in Update() to safely dispatch network events to Unity Main Thread.
        /// </summary>
        public void Update()
        {
            _socket?.Poll();

            while (_inboundQueue.TryDequeue(out byte[] raw))
            {
                // Strip 7-byte WebSocket framing (0x11 0xA5 "delta") or 6-byte UDP framing (0xA5 "delta")
                int startOffset = 0;
                if (raw.Length >= 7 && raw[0] == 0x11 && raw[1] == 0xA5 &&
                    raw[2] == (byte)'d' && raw[3] == (byte)'e' && raw[4] == (byte)'l' && raw[5] == (byte)'t' && raw[6] == (byte)'a')
                {
                    startOffset = 7;
                }
                else if (raw.Length >= 6 && raw[0] == 0xA5 &&
                    raw[1] == (byte)'d' && raw[2] == (byte)'e' && raw[3] == (byte)'l' && raw[4] == (byte)'t' && raw[5] == (byte)'a')
                {
                    startOffset = 6;
                }
                else
                {
                    continue;
                }

                // If match recording is active, capture frame
                if (_isRecording)
                {
                    int timeMs = Mathf.RoundToInt((Time.time - _recordStartTime) * 1000f);
                    _recordedFrames.Add(new ReplayFrameData
                    {
                        t = timeMs,
                        d = raw
                    });
                }

                OnRawPacket?.Invoke(raw);

                StateDelta delta = StateDelta.Unpack(raw, startOffset);
                if (delta != null)
                {
                    LocalPlayerId = delta.LocalPlayerId;
                    OnDelta?.Invoke(delta);
                }
            }
        }

        /// <summary>
        /// Stream a 20-byte packed player input frame to the authoritative server.
        /// </summary>
        public void SendInput(uint flags, float aimYaw, float aimPitch, float moveX, float moveZ)
        {
            if (IsSpectator || _socket == null || !_socket.IsConnected) return;

            PlayerInput input = new PlayerInput
            {
                Flags = flags,
                AimYaw = aimYaw,
                AimPitch = aimPitch,
                MoveX = moveX,
                MoveZ = moveZ
            };

            _socket.SendInput(input);
        }

        #region Replay Recording (.oxr format)
        public void StartRecording()
        {
            _isRecording = true;
            _recordStartTime = Time.time;
            _recordedFrames.Clear();
            Debug.Log($"[OpenOxide] ⏺️ Started match recording for room '{RoomName}'");
        }

        public MatchReplayData StopRecording()
        {
            _isRecording = false;
            int duration = Mathf.RoundToInt((Time.time - _recordStartTime) * 1000f);
            Debug.Log($"[OpenOxide] ⏹️ Recording finished: {_recordedFrames.Count} frames ({duration}ms)");

            return new MatchReplayData
            {
                room = RoomName,
                recordedAt = DateTime.UtcNow.ToString("o"),
                durationMs = duration,
                totalFrames = _recordedFrames.Count,
                frames = new List<ReplayFrameData>(_recordedFrames)
            };
        }

        public void SaveReplayToFile(string filePath)
        {
            MatchReplayData data = StopRecording();
            string json = JsonUtility.ToJson(data, true);
            File.WriteAllText(filePath, json);
            Debug.Log($"[OpenOxide] 💾 Replay saved to: {filePath}");
        }
        #endregion

        public void Leave()
        {
            _socket?.Disconnect();
        }

        public void Dispose()
        {
            Leave();
            _socket?.Dispose();
        }
    }
}
