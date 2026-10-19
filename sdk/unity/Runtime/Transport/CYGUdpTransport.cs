using System;
using System.Buffers;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;

namespace CookYourGame
{
    /// <summary>
    /// High-performance Raw UDP Transport connecting to OpenOxide's 128 Hz MIO plane.
    /// Eliminates TCP Head-of-Line blocking, delivering sub-millisecond datagrams
    /// for transforms, RPCs, SyncVars and spawns.
    /// </summary>
    public sealed class CYGUdpTransport : CYGTransport
    {
        UdpClient socket;
        IPEndPoint remoteEndpoint;
        CancellationTokenSource cancellation;
        Task receiveTask;
        string currentAuthToken;

        public override void Connect(string url, string authToken)
        {
            Close();
            ResetEventQueue();

            currentAuthToken = authToken;

            try
            {
                string host = "127.0.0.1";
                int port = 7778;

                if (!string.IsNullOrEmpty(url))
                {
                    if (Uri.TryCreate(url, UriKind.Absolute, out Uri uri))
                    {
                        host = uri.Host;
                        port = uri.Port > 0 ? uri.Port : 7778;
                    }
                    else
                    {
                        string clean = url.Replace("udp://", "").Replace("http://", "").Replace("ws://", "");
                        int colonIdx = clean.IndexOf(':');
                        if (colonIdx >= 0)
                        {
                            host = clean.Substring(0, colonIdx);
                            int.TryParse(clean.Substring(colonIdx + 1).Split('/')[0], out port);
                        }
                        else
                        {
                            host = clean;
                        }
                    }
                }

                if (port <= 0) port = 7778;

                IPAddress[] ips = Dns.GetHostAddresses(host);
                IPAddress targetIp = (ips != null && ips.Length > 0) ? ips[0] : IPAddress.Loopback;
                remoteEndpoint = new IPEndPoint(targetIp, port);

                socket = new UdpClient();
                socket.Client.SendBufferSize = 1024 * 1024;
                socket.Client.ReceiveBufferSize = 1024 * 1024;
                socket.Connect(remoteEndpoint);

                cancellation = new CancellationTokenSource();
                IsConnected = true;

                // Send initial handshake datagram to bind peer on MIO plane
                SendHandshake(authToken);

                CancellationToken token = cancellation.Token;
                receiveTask = Task.Run(() => ReceiveLoop(socket, token), token);

                RaiseConnected();
                Debug.Log($"[CYG UDP] Connected successfully to MIO UDP plane at {remoteEndpoint}");
            }
            catch (Exception error)
            {
                IsConnected = false;
                Debug.LogError($"[CYG UDP] Failed to connect to {url}: {error.Message}");
                RaiseDisconnected("UDP connect failed: " + error.Message);
            }
        }

        void SendHandshake(string token)
        {
            if (socket == null || !IsConnected) return;

            try
            {
                byte[] tokenBytes = string.IsNullOrEmpty(token) ? Array.Empty<byte>() : Encoding.UTF8.GetBytes(token);
                // WireJoinRoom (10) framing
                byte[] joinPkt = new byte[1 + tokenBytes.Length];
                joinPkt[0] = CYGWireProtocol.WireJoinRoom;
                if (tokenBytes.Length > 0)
                {
                    Buffer.BlockCopy(tokenBytes, 0, joinPkt, 1, tokenBytes.Length);
                }

                socket.Send(joinPkt, joinPkt.Length);
            }
            catch (Exception ex)
            {
                Debug.LogWarning("[CYG UDP] Handshake send warning: " + ex.Message);
            }
        }

        async Task ReceiveLoop(UdpClient target, CancellationToken token)
        {
            while (!token.IsCancellationRequested && target != null)
            {
                try
                {
                    UdpReceiveResult result = await target.ReceiveAsync();
                    byte[] buffer = result.Buffer;
                    if (buffer != null && buffer.Length > 0)
                    {
                        // UDP delivers full discrete datagrams; deframe immediately
                        AppendReceived(new ArraySegment<byte>(buffer, 0, buffer.Length), true);
                    }
                }
                catch (ObjectDisposedException)
                {
                    break;
                }
                catch (SocketException se)
                {
                    if (token.IsCancellationRequested) break;
                    Debug.LogWarning("[CYG UDP] socket receive warning: " + se.Message);
                    RaiseDisconnected("UDP socket error: " + se.Message);
                    break;
                }
                catch (Exception error)
                {
                    if (token.IsCancellationRequested) break;
                    Debug.LogWarning("[CYG UDP] receive loop exception: " + error.Message);
                }
            }
        }

        public override void Send(ArraySegment<byte> framedPacket)
        {
            if (framedPacket.Array == null || framedPacket.Count < CYGWireProtocol.HeaderSize)
            {
                return;
            }

            if (socket == null || !IsConnected)
            {
                Debug.LogWarning("[CYG UDP] Dropped outbound packet: socket is not connected.");
                return;
            }

            byte rawOp = framedPacket.Array[framedPacket.Offset + 2];
            CYGOpCode op = (CYGOpCode)rawOp;
            bool tunneled = CYGWireProtocol.IsTunneledOp(op);

            int payloadLength = (framedPacket.Array[framedPacket.Offset] << 8) | framedPacket.Array[framedPacket.Offset + 1];
            int wireLength = tunneled ? framedPacket.Count + 2 : payloadLength + 1;

            byte[] wire = ArrayPool<byte>.Shared.Rent(wireLength);
            try
            {
                wire[0] = CYGWireProtocol.ToWireCode(op);
                if (tunneled)
                {
                    wire[1] = CYGWireProtocol.TunnelMagic;
                    Buffer.BlockCopy(framedPacket.Array, framedPacket.Offset, wire, 2, framedPacket.Count);
                }
                else
                {
                    Buffer.BlockCopy(framedPacket.Array, framedPacket.Offset + CYGWireProtocol.HeaderSize, wire, 1, payloadLength);
                }

                // Send directly via zero-allocation non-blocking UDP datagram - zero queue delay!
                socket.Send(wire, wireLength);
            }
            catch (Exception error)
            {
                Debug.LogWarning("[CYG UDP] Send failed: " + error.Message);
            }
            finally
            {
                ArrayPool<byte>.Shared.Return(wire);
            }
        }

        /// <summary>Sends a raw datagram directly to the UDP socket with zero framing overhead.</summary>
        public void SendRaw(ArraySegment<byte> segment)
        {
            if (socket == null || !IsConnected || segment.Array == null || segment.Count == 0) return;
            try
            {
                if (segment.Offset == 0 && segment.Count == segment.Array.Length)
                {
                    socket.Send(segment.Array, segment.Count);
                }
                else
                {
                    byte[] temp = ArrayPool<byte>.Shared.Rent(segment.Count);
                    try
                    {
                        Buffer.BlockCopy(segment.Array, segment.Offset, temp, 0, segment.Count);
                        socket.Send(temp, segment.Count);
                    }
                    finally
                    {
                        ArrayPool<byte>.Shared.Return(temp);
                    }
                }
            }
            catch (Exception error)
            {
                Debug.LogWarning("[CYG UDP] SendRaw failed: " + error.Message);
            }
        }

        /// <summary>Sends a raw datagram directly to the UDP socket with zero framing overhead.</summary>
        public void SendRaw(byte[] buffer, int offset, int count)
        {
            SendRaw(new ArraySegment<byte>(buffer, offset, count));
        }

        public override void Close()
        {
            CancellationTokenSource src = cancellation;
            cancellation = null;
            if (src != null)
            {
                try { src.Cancel(); } catch { }
                src.Dispose();
            }

            if (socket != null)
            {
                try { socket.Close(); } catch { }
                try { socket.Dispose(); } catch { }
                socket = null;
            }

            IsConnected = false;
        }
    }
}
