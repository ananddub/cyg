using System;
using UnityEngine;
#if UNITY_WEBGL && !UNITY_EDITOR
using System.Runtime.InteropServices;
#else
using System.Net.WebSockets;
using System.Threading;
using System.Threading.Tasks;
#endif

namespace OpenOxide
{
    /// <summary>
    /// Binary WebSocket transport for OpenOxide client.
    /// Uses native browser WebSocket API in WebGL with thread-safe main thread polling,
    /// and ClientWebSocket in Editor / Standalone.
    /// </summary>
    public class OpenOxideWebSocket : IOpenOxideSocket
    {
        public bool IsConnected { get; private set; }
        public event Action<byte[]> OnPacketReceived;
        public event Action<Exception> OnError;

#if UNITY_WEBGL && !UNITY_EDITOR
        [DllImport("__Internal")]
        private static extern int OpenOxide_WS_Connect(string url);

        [DllImport("__Internal")]
        private static extern int OpenOxide_WS_GetState(int socketId);

        [DllImport("__Internal")]
        private static extern int OpenOxide_WS_PollPacketSize(int socketId);

        [DllImport("__Internal")]
        private static extern int OpenOxide_WS_PollPacket(int socketId, [In, Out] byte[] dstPtr, int maxLen);

        [DllImport("__Internal")]
        private static extern void OpenOxide_WS_Send(int socketId, byte[] ptr, int length);

        [DllImport("__Internal")]
        private static extern void OpenOxide_WS_Close(int socketId);

        private int _socketId = -1;
        private byte[] _pollBuffer = new byte[65536];

        public void Connect(string url)
        {
            Disconnect();
            _socketId = OpenOxide_WS_Connect(url);
            if (_socketId < 0)
            {
                OnError?.Invoke(new Exception("Failed to initialize browser WebSocket"));
            }
        }

        public void Poll()
        {
            if (_socketId < 0) return;

            int state = OpenOxide_WS_GetState(_socketId);
            if (state == 1 && !IsConnected)
            {
                IsConnected = true;
            }
            else if (state == 3 && IsConnected)
            {
                IsConnected = false;
            }
            else if (state == 4)
            {
                IsConnected = false;
                OnError?.Invoke(new Exception("WebSocket reported error"));
                return;
            }

            while (_socketId >= 0)
            {
                int size = OpenOxide_WS_PollPacketSize(_socketId);
                if (size <= 0) break;

                if (_pollBuffer.Length < size)
                {
                    _pollBuffer = new byte[size * 2];
                }

                int read = OpenOxide_WS_PollPacket(_socketId, _pollBuffer, _pollBuffer.Length);
                if (read > 0)
                {
                    byte[] packet = new byte[read];
                    Buffer.BlockCopy(_pollBuffer, 0, packet, 0, read);
                    OnPacketReceived?.Invoke(packet);
                }
                else
                {
                    break;
                }
            }
        }

        public void Send(byte[] data)
        {
            if (!IsConnected || _socketId < 0 || data == null) return;
            OpenOxide_WS_Send(_socketId, data, data.Length);
        }

        public void Disconnect()
        {
            if (_socketId >= 0)
            {
                OpenOxide_WS_Close(_socketId);
                _socketId = -1;
            }
            IsConnected = false;
        }
#else
        private ClientWebSocket _client;
        private CancellationTokenSource _cts;

        public void Poll()
        {
            // Background Task loop handles packet reception on Standalone / Editor
        }

        public void Connect(string url)
        {
            Disconnect();
            _client = new ClientWebSocket();
            _cts = new CancellationTokenSource();

            Task.Run(async () =>
            {
                try
                {
                    Uri uri = new Uri(url);
                    await _client.ConnectAsync(uri, _cts.Token);
                    IsConnected = true;
                    await ReceiveLoop(_cts.Token);
                }
                catch (Exception ex)
                {
                    IsConnected = false;
                    OnError?.Invoke(ex);
                }
            });
        }

        private async Task ReceiveLoop(CancellationToken token)
        {
            byte[] buffer = new byte[65536];
            while (!token.IsCancellationRequested && _client != null && _client.State == WebSocketState.Open)
            {
                try
                {
                    var seg = new ArraySegment<byte>(buffer);
                    var result = await _client.ReceiveAsync(seg, token);
                    if (result.MessageType == WebSocketMessageType.Close)
                    {
                        break;
                    }

                    if (result.Count > 0)
                    {
                        byte[] packet = new byte[result.Count];
                        Array.Copy(buffer, 0, packet, 0, result.Count);
                        OnPacketReceived?.Invoke(packet);
                    }
                }
                catch (Exception ex)
                {
                    if (!token.IsCancellationRequested)
                    {
                        OnError?.Invoke(ex);
                    }
                    break;
                }
            }
            IsConnected = false;
        }

        public void Send(byte[] data)
        {
            if (!IsConnected || _client == null || _client.State != WebSocketState.Open || data == null) return;
            try
            {
                var seg = new ArraySegment<byte>(data);
                _client.SendAsync(seg, WebSocketMessageType.Binary, true, CancellationToken.None);
            }
            catch (Exception ex)
            {
                OnError?.Invoke(ex);
            }
        }

        public void Disconnect()
        {
            IsConnected = false;
            _cts?.Cancel();
            try { _client?.Dispose(); } catch { }
            _client = null;
        }
#endif

        public void SendInput(PlayerInput input)
        {
            byte[] bytes = input.ToBytes();
            Send(bytes);
        }

        public void Dispose()
        {
            Disconnect();
        }
    }
}
