using System;
using System.Net;
using System.Net.Sockets;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;

namespace OpenOxide
{
    /// <summary>
    /// High-throughput UDP datagram socket for OpenOxide game client.
    /// Handles asynchronous non-blocking packet reception and transmission.
    /// </summary>
    public class OpenOxideUdpSocket : IOpenOxideSocket
    {
        private UdpClient _udpClient;
        private IPEndPoint _serverEndPoint;
        private CancellationTokenSource _cts;
        private bool _isRunning;

        public event Action<byte[]> OnPacketReceived;
        public event Action<Exception> OnError;

        public bool IsConnected => _isRunning && _udpClient != null;

        public void Connect(string host, int port)
        {
            if (_isRunning) Disconnect();

            try
            {
                IPAddress ip;
                if (!IPAddress.TryParse(host, out ip))
                {
                    IPAddress[] addresses = Dns.GetHostAddresses(host);
                    if (addresses.Length == 0) throw new Exception($"Could not resolve host: {host}");
                    ip = addresses[0];
                }

                _serverEndPoint = new IPEndPoint(ip, port);
                _udpClient = new UdpClient();
                _udpClient.Connect(_serverEndPoint);

                _cts = new CancellationTokenSource();
                _isRunning = true;

                Task.Run(() => ReceiveLoop(_cts.Token));
            }
            catch (Exception ex)
            {
                OnError?.Invoke(ex);
                throw;
            }
        }

        private async Task ReceiveLoop(CancellationToken token)
        {
            while (!token.IsCancellationRequested && _isRunning)
            {
                try
                {
                    UdpReceiveResult result = await _udpClient.ReceiveAsync();
                    if (result.Buffer != null && result.Buffer.Length > 0)
                    {
                        OnPacketReceived?.Invoke(result.Buffer);
                    }
                }
                catch (ObjectDisposedException)
                {
                    break;
                }
                catch (Exception ex)
                {
                    if (!token.IsCancellationRequested)
                    {
                        OnError?.Invoke(ex);
                    }
                }
            }
        }

        public void Send(byte[] data)
        {
            if (!_isRunning || _udpClient == null || data == null) return;

            try
            {
                _udpClient.Send(data, data.Length);
            }
            catch (Exception ex)
            {
                OnError?.Invoke(ex);
            }
        }

        public void SendInput(PlayerInput input)
        {
            byte[] bytes = input.ToBytes();
            Send(bytes);
        }

        public void Disconnect()
        {
            _isRunning = false;
            _cts?.Cancel();
            _udpClient?.Close();
            _udpClient = null;
        }

        public void Dispose()
        {
            Disconnect();
            _cts?.Dispose();
        }
    }
}
