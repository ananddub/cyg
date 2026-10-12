using System;
using System.Collections.Generic;
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
    /// Uses native browser WebSocket API in WebGL via .jslib, and ClientWebSocket in Editor / Standalone.
    /// </summary>
    public class OpenOxideWebSocket : IOpenOxideSocket
    {
        public bool IsConnected { get; private set; }
        public event Action<byte[]> OnPacketReceived;
        public event Action<Exception> OnError;

#if UNITY_WEBGL && !UNITY_EDITOR
        private delegate void OnOpenDelegate(int socketId);
        private delegate void OnMessageDelegate(int socketId, IntPtr ptr, int length);
        private delegate void OnErrorDelegate(int socketId);
        private delegate void OnCloseDelegate(int socketId);

        [DllImport("__Internal")]
        private static extern int OpenOxide_WS_Connect(string url, OnOpenDelegate onOpen, OnMessageDelegate onMsg, OnErrorDelegate onErr, OnCloseDelegate onClose);

        [DllImport("__Internal")]
        private static extern void OpenOxide_WS_Send(int socketId, byte[] ptr, int length);

        [DllImport("__Internal")]
        private static extern void OpenOxide_WS_Close(int socketId);

        private static readonly Dictionary<int, OpenOxideWebSocket> _instances = new Dictionary<int, OpenOxideWebSocket>();
        private int _socketId = -1;

        [AOT.MonoPInvokeCallback(typeof(OnOpenDelegate))]
        private static void StaticOnOpen(int id)
        {
            if (_instances.TryGetValue(id, out var ws))
            {
                ws.IsConnected = true;
            }
        }

        [AOT.MonoPInvokeCallback(typeof(OnMessageDelegate))]
        private static void StaticOnMessage(int id, IntPtr ptr, int len)
        {
            if (_instances.TryGetValue(id, out var ws))
            {
                byte[] buffer = new byte[len];
                Marshal.Copy(ptr, buffer, 0, len);
                ws.OnPacketReceived?.Invoke(buffer);
            }
        }

        [AOT.MonoPInvokeCallback(typeof(OnErrorDelegate))]
        private static void StaticOnError(int id)
        {
            if (_instances.TryGetValue(id, out var ws))
            {
                ws.OnError?.Invoke(new Exception("WebSocket error reported by browser"));
            }
        }

        [AOT.MonoPInvokeCallback(typeof(OnCloseDelegate))]
        private static void StaticOnClose(int id)
        {
            if (_instances.TryGetValue(id, out var ws))
            {
                ws.IsConnected = false;
                _instances.Remove(id);
            }
        }

        public void Connect(string url)
        {
            Disconnect();
            _socketId = OpenOxide_WS_Connect(url, StaticOnOpen, StaticOnMessage, StaticOnError, StaticOnClose);
            if (_socketId >= 0)
            {
                _instances[_socketId] = this;
            }
            else
            {
                OnError?.Invoke(new Exception("Failed to initialize browser WebSocket"));
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
                _instances.Remove(_socketId);
                _socketId = -1;
            }
            IsConnected = false;
        }
#else
        private ClientWebSocket _client;
        private CancellationTokenSource _cts;

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
