using System;
using System.Buffers;
using System.Collections.Generic;
using System.Net.WebSockets;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;

namespace CookYourGame
{
    internal enum CYGQueueKind : byte
    {
        Connected = 0,
        Disconnected = 1,
        Frame = 2,
    }

    internal struct CYGQueueSlot
    {
        public CYGQueueKind Kind;
        public byte[] Buffer;
        public int Offset;
        public int Length;
        public string Text;
    }

    /// <summary>Wire-code and canonical-frame translation for the CYG packet header.</summary>
    internal static class CYGWireProtocol
    {
        public const int HeaderSize = 3;
        public const byte CodeMask = 0x1F;

        public const byte WireJoinRoom = 10;
        public const byte WireError = 11;
        public const byte WireLeaveRoom = 12;
        public const byte WireRoomData = 13;
        public const byte WireRoomState = 14;
        public const byte WireRoomStatePatch = 15;
        public const byte WireRoomDataBytes = 17;
        public const byte WirePing = 18;
        public const byte WireRoomRequest = 21;
        public const byte WireRoomResponse = 22;

        /// <summary>
        /// Prefix inside ROOM_DATA_BYTES that marks the rest of the message as a
        /// canonical CYG packet. A distinct byte keeps the tunnel apart from the
        /// msgpack, voice and delta blobs the room server also sends there.
        /// </summary>
        public const byte TunnelMagic = 0x43;

        /// <summary>Opcodes that may appear in the canonical CYG packet header.</summary>
        public static bool IsCanonicalOp(byte rawOp)
        {
            switch (rawOp)
            {
                case 0:  // Ping
                case 1:  // Pong
                case 2:  // JoinRoom
                case 3:  // LeaveRoom
                case 4:  // PlayerInput
                case 5:  // StateDelta
                case 6:  // CustomRpc
                case 7:  // RewindHit
                case 8:  // StateSnapshot
                case 10: // ProtoJoinRoom
                case 12: // ProtoLeaveRoom
                case 13: // ProtoRoomData
                case 14: // ProtoRoomState
                case 15: // ProtoStatePatch
                case 17: // ProtoDataBytes
                case 20: // SpawnObject
                case 21: // DespawnObject
                    return true;
                default:
                    return false;
            }
        }

        /// <summary>Game-layer messages the engine relays as opaque room bytes.</summary>
        public static bool IsTunneledOp(CYGOpCode op)
        {
            return op == CYGOpCode.CustomRpc
                || op == CYGOpCode.RewindHit
                || op == CYGOpCode.SpawnObject
                || op == CYGOpCode.DespawnObject;
        }

        public static byte ToWireCode(CYGOpCode op)
        {
            switch (op)
            {
                case CYGOpCode.Ping:
                case CYGOpCode.Pong:
                    return WirePing;
                case CYGOpCode.JoinRoom:
                case CYGOpCode.ProtoJoinRoom:
                    return WireJoinRoom;
                case CYGOpCode.LeaveRoom:
                case CYGOpCode.ProtoLeaveRoom:
                    return WireLeaveRoom;
                case CYGOpCode.PlayerInput:
                case CYGOpCode.ProtoRoomData:
                    return WireRoomData;
                case CYGOpCode.StateDelta:
                case CYGOpCode.ProtoStatePatch:
                    return WireRoomStatePatch;
                case CYGOpCode.StateSnapshot:
                case CYGOpCode.ProtoRoomState:
                    return WireRoomState;
                default:
                    return WireRoomDataBytes;
            }
        }

        public static bool TryFromWireCode(byte code, out CYGOpCode op)
        {
            switch (code)
            {
                case WirePing:
                    op = CYGOpCode.Pong;
                    return true;
                case WireJoinRoom:
                    op = CYGOpCode.ProtoJoinRoom;
                    return true;
                case WireLeaveRoom:
                    op = CYGOpCode.ProtoLeaveRoom;
                    return true;
                case WireRoomData:
                    op = CYGOpCode.ProtoRoomData;
                    return true;
                case WireRoomState:
                    op = CYGOpCode.ProtoRoomState;
                    return true;
                case WireRoomStatePatch:
                    op = CYGOpCode.ProtoStatePatch;
                    return true;
                case WireRoomDataBytes:
                    op = CYGOpCode.ProtoDataBytes;
                    return true;
                default:
                    op = default(CYGOpCode);
                    return false;
            }
        }
    }

    /// <summary>
    /// Packet transport. Inbound bytes are deframed into canonical CYG packets
    /// ([len u16 BE][op u8][payload]) and every event is raised on the Unity main thread.
    /// </summary>
    public abstract class CYGTransport
    {
        const int RingSize = 512;
        const int RingMask = RingSize - 1;
        const int InitialReceiveCapacity = 16384;
        const int MaxReceiveCapacity = 1024 * 1024;

        readonly CYGQueueSlot[] ring = new CYGQueueSlot[RingSize];
        int producerIndex;
        int consumerIndex;

        byte[] receiveScratch = ArrayPool<byte>.Shared.Rent(InitialReceiveCapacity);
        int receiveLength;

        /// <summary>Raised once the socket is open and the room handshake may start.</summary>
        public event Action OnConnected;

        /// <summary>Raised when the link drops, with a human-readable reason.</summary>
        public event Action<string> OnDisconnected;

        /// <summary>Raised with one complete packet, header included.</summary>
        public event Action<ArraySegment<byte>> OnFrame;

        /// <summary>True while the link is usable.</summary>
        public bool IsConnected { get; protected set; }

        /// <summary>Opens the link to a room endpoint, presenting an optional seat token.</summary>
        public abstract void Connect(string url, string authToken);

        /// <summary>Sends one fully framed packet.</summary>
        public abstract void Send(ArraySegment<byte> framedPacket);

        /// <summary>Tears the link down and releases socket resources.</summary>
        public abstract void Close();

        /// <summary>Drains the inbound queue and flushes queued outbound frames on the main thread.</summary>
        public void Pump()
        {
            DrainEventQueue();
            OnMainThreadPump();
        }

        /// <summary>Hook for subclasses that need per-frame main-thread work.</summary>
        protected virtual void OnMainThreadPump()
        {
        }

        protected void RaiseConnected()
        {
            Enqueue(CYGQueueKind.Connected, null, 0, 0, null);
        }

        protected void RaiseDisconnected(string reason)
        {
            IsConnected = false;
            Enqueue(CYGQueueKind.Disconnected, null, 0, 0, reason ?? "disconnected");
        }

        protected void RaiseFrame(ArraySegment<byte> packet)
        {
            // The caller keeps ownership of its buffer, so the queue stores a copy.
            EnqueueFrame(packet.Array, packet.Offset, packet.Count);
        }

        /// <summary>Appends socket bytes and deframes once a message boundary is reached.</summary>
        protected void AppendReceived(ArraySegment<byte> chunk, bool endOfMessage)
        {
            if (chunk.Array == null || chunk.Count == 0)
            {
                return;
            }

            if (receiveScratch.Length == 0)
            {
                receiveScratch = ArrayPool<byte>.Shared.Rent(InitialReceiveCapacity);
            }

            if (chunk.Count > MaxReceiveCapacity - receiveLength)
            {
                Debug.LogError("[CYG] inbound message exceeded the transport buffer; the link is reset.");
                receiveLength = 0;
                return;
            }

            if (receiveLength + chunk.Count > receiveScratch.Length)
            {
                int capacity = receiveScratch.Length;
                while (capacity < receiveLength + chunk.Count)
                {
                    capacity <<= 1;
                }

                byte[] grown = ArrayPool<byte>.Shared.Rent(capacity);
                Buffer.BlockCopy(receiveScratch, 0, grown, 0, receiveLength);
                ArrayPool<byte>.Shared.Return(receiveScratch);
                receiveScratch = grown;
            }

            Buffer.BlockCopy(chunk.Array, chunk.Offset, receiveScratch, receiveLength, chunk.Count);
            receiveLength += chunk.Count;

            if (!endOfMessage)
            {
                return;
            }

            try
            {
                DeframeMessage(receiveScratch, receiveLength);
            }
            catch (Exception error)
            {
                Debug.LogError("[CYG] deframe failed: " + error.Message);
            }

            receiveLength = 0;
        }

        /// <summary>Resets the per-message accumulation, e.g. after a transport error.</summary>
        protected void ResetReceiveAccumulator()
        {
            receiveLength = 0;
        }

        /// <summary>
        /// Drops queued events from a previous link. Only safe from the main thread
        /// while no producer is live, i.e. before a new socket is opened.
        /// </summary>
        protected void ResetEventQueue()
        {
            while (Volatile.Read(ref producerIndex) != consumerIndex)
            {
                CYGQueueSlot stale = ring[consumerIndex];
                consumerIndex = (consumerIndex + 1) & RingMask;
                if (stale.Buffer != null)
                {
                    ArrayPool<byte>.Shared.Return(stale.Buffer);
                }
            }

            Volatile.Write(ref producerIndex, consumerIndex);
            receiveLength = 0;
        }

        void DeframeMessage(byte[] data, int length)
        {
            if (length == 0)
            {
                return;
            }

            byte leading = (byte)(data[0] & CYGWireProtocol.CodeMask);
            CYGOpCode op;

            // Room payloads ride ROOM_DATA_BYTES as [17]['C'][canonical packet].
            if (leading == CYGWireProtocol.WireRoomDataBytes && length > 2 && data[1] == CYGWireProtocol.TunnelMagic)
            {
                int frames;
                if (TryCountCanonicalFrames(data, 2, length - 2, out frames))
                {
                    int position = 2;
                    for (int i = 0; i < frames; i++)
                    {
                        int payloadLength = (data[position] << 8) | data[position + 1];
                        EnqueueFrame(data, position, CYGWireProtocol.HeaderSize + payloadLength);
                        position += CYGWireProtocol.HeaderSize + payloadLength;
                    }

                    return;
                }
            }

            // The room server frames one wire message as [opcode byte][payload]; the
            // message length comes from the socket, so the CYG header is synthesized.
            if (CYGWireProtocol.TryFromWireCode(leading, out op))
            {
                EnqueueNormalized(op, data, 1, length - 1);
                return;
            }

            int counted;
            if (TryCountCanonicalFrames(data, length, out counted))
            {
                int position = 0;
                for (int i = 0; i < counted; i++)
                {
                    int payloadLength = (data[position] << 8) | data[position + 1];
                    EnqueueFrame(data, position, CYGWireProtocol.HeaderSize + payloadLength);
                    position += CYGWireProtocol.HeaderSize + payloadLength;
                }

                return;
            }

            Debug.LogWarning("[CYG] could not deframe a " + length + "-byte message (leading byte " + leading + ").");
        }

        static bool TryCountCanonicalFrames(byte[] data, int length, out int frames)
        {
            return TryCountCanonicalFrames(data, 0, length, out frames);
        }

        static bool TryCountCanonicalFrames(byte[] data, int start, int length, out int frames)
        {
            frames = 0;
            int position = start;
            int end = start + length;

            while (position < end)
            {
                if (end - position < CYGWireProtocol.HeaderSize)
                {
                    return false;
                }

                int payloadLength = (data[position] << 8) | data[position + 1];
                int total = CYGWireProtocol.HeaderSize + payloadLength;
                if (position + total > end)
                {
                    return false;
                }

                if (!CYGWireProtocol.IsCanonicalOp(data[position + 2]))
                {
                    return false;
                }

                position += total;
                frames++;
            }

            return frames > 0;
        }

        void EnqueueFrame(byte[] source, int offset, int totalLength)
        {
            byte[] copy = ArrayPool<byte>.Shared.Rent(totalLength);
            Buffer.BlockCopy(source, offset, copy, 0, totalLength);
            Enqueue(CYGQueueKind.Frame, copy, 0, totalLength, null);
        }

        void EnqueueNormalized(CYGOpCode op, byte[] source, int payloadOffset, int payloadLength)
        {
            int totalLength = CYGWireProtocol.HeaderSize + payloadLength;
            byte[] copy = ArrayPool<byte>.Shared.Rent(totalLength);
            copy[0] = (byte)((payloadLength >> 8) & 0xFF);
            copy[1] = (byte)(payloadLength & 0xFF);
            copy[2] = (byte)op;
            if (payloadLength > 0)
            {
                Buffer.BlockCopy(source, payloadOffset, copy, CYGWireProtocol.HeaderSize, payloadLength);
            }

            Enqueue(CYGQueueKind.Frame, copy, 0, totalLength, null);
        }

        void Enqueue(CYGQueueKind kind, byte[] buffer, int offset, int length, string text)
        {
            int head = producerIndex;
            int next = (head + 1) & RingMask;
            if (next == Volatile.Read(ref consumerIndex))
            {
                Debug.LogWarning("[CYG] inbound queue overflow; a " + kind + " event was dropped.");
                if (buffer != null)
                {
                    ArrayPool<byte>.Shared.Return(buffer);
                }

                return;
            }

            CYGQueueSlot slot;
            slot.Kind = kind;
            slot.Buffer = buffer;
            slot.Offset = offset;
            slot.Length = length;
            slot.Text = text;
            ring[head] = slot;
            Volatile.Write(ref producerIndex, next);
        }

        void DrainEventQueue()
        {
            while (Volatile.Read(ref producerIndex) != consumerIndex)
            {
                CYGQueueSlot slot = ring[consumerIndex];
                consumerIndex = (consumerIndex + 1) & RingMask;

                switch (slot.Kind)
                {
                    case CYGQueueKind.Connected:
                        IsConnected = true;
                        if (OnConnected != null)
                        {
                            OnConnected.Invoke();
                        }

                        break;
                    case CYGQueueKind.Disconnected:
                        IsConnected = false;
                        if (OnDisconnected != null)
                        {
                            OnDisconnected.Invoke(slot.Text);
                        }

                        break;
                    case CYGQueueKind.Frame:
                        if (OnFrame != null)
                        {
                            OnFrame.Invoke(new ArraySegment<byte>(slot.Buffer, slot.Offset, slot.Length));
                        }

                        break;
                }

                if (slot.Buffer != null)
                {
                    ArrayPool<byte>.Shared.Return(slot.Buffer);
                }
            }
        }
    }

    /// <summary>WebSocket transport for standalone, mobile and WebGL builds.</summary>
    public sealed class CYGWebSocketTransport : CYGTransport
    {
        const int MaxOutstandingFrames = 128;
        const int ReceiveChunkSize = 16384;
        const int MaxWireMessageSize = 65536;

        readonly Queue<ArraySegment<byte>> outbound = new Queue<ArraySegment<byte>>(64);

        ClientWebSocket socket;
        CancellationTokenSource cancellation;
        Task pendingSend;
        byte[] pendingSendBuffer;

        /// <summary>Opens the socket and starts the asynchronous receive loop.</summary>
        public override void Connect(string url, string authToken)
        {
            if (string.IsNullOrEmpty(url))
            {
                Debug.LogError("[CYG] Connect requires a non-empty websocket url.");
                return;
            }

            Close();
            ResetEventQueue();

            try
            {
                cancellation = new CancellationTokenSource();
                socket = new ClientWebSocket();
                socket.Options.KeepAliveInterval = TimeSpan.FromSeconds(15);
                if (!string.IsNullOrEmpty(authToken))
                {
                    try
                    {
                        socket.Options.SetRequestHeader("Authorization", "Bearer " + authToken);
                    }
                    catch (Exception headerError)
                    {
                        // WebGL ignores custom handshake headers: the seat token also travels in the query string.
                        Debug.LogWarning("[CYG] could not set the Authorization header: " + headerError.Message);
                    }
                }
            }
            catch (Exception error)
            {
                Debug.LogError("[CYG] websocket initialization failed: " + error.Message);
                DisposeSocket();
                RaiseDisconnected(error.Message);
                return;
            }

            StartConnection(url);
        }

        /// <summary>Frames and queues one CYG packet for the room server.</summary>
        public override void Send(ArraySegment<byte> framedPacket)
        {
            if (framedPacket.Array == null || framedPacket.Count < CYGWireProtocol.HeaderSize)
            {
                Debug.LogError("[CYG] Send received a packet shorter than the 3-byte header.");
                return;
            }

            byte rawOp = framedPacket.Array[framedPacket.Offset + 2];
            if (!CYGWireProtocol.IsCanonicalOp(rawOp))
            {
                Debug.LogError("[CYG] Send received an unsendable opcode " + rawOp + ".");
                return;
            }

            int payloadLength = framedPacket.Count - CYGWireProtocol.HeaderSize;
            CYGOpCode op = (CYGOpCode)rawOp;
            bool tunneled = CYGWireProtocol.IsTunneledOp(op);
            int wireLength = tunneled
                ? 2 + framedPacket.Count
                : 1 + payloadLength;
            if (wireLength > MaxWireMessageSize)
            {
                Debug.LogError("[CYG] packet too large for one websocket message: " + wireLength + " bytes.");
                return;
            }

            if (socket == null || socket.State != WebSocketState.Open)
            {
                Debug.LogWarning("[CYG] dropped an outbound packet: the socket is not open.");
                return;
            }

            if (outbound.Count >= MaxOutstandingFrames)
            {
                Debug.LogWarning("[CYG] send queue is saturated; dropping a " + op + " packet.");
                return;
            }

            byte[] wire = ArrayPool<byte>.Shared.Rent(wireLength);
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

            outbound.Enqueue(new ArraySegment<byte>(wire, 0, wireLength));
        }

        /// <summary>Cancels the receive loop and aborts the socket.</summary>
        public override void Close()
        {
            CancellationTokenSource source = cancellation;
            cancellation = null;
            if (source != null)
            {
                try
                {
                    source.Cancel();
                }
                catch (Exception error)
                {
                    Debug.LogWarning("[CYG] cancel failed: " + error.Message);
                }

                source.Dispose();
            }

            ClearOutbound();
            DisposeSocket();
        }

        protected override void OnMainThreadPump()
        {
            if (socket == null || outbound.Count == 0)
            {
                return;
            }

            if (pendingSend != null)
            {
                if (!pendingSend.IsCompleted)
                {
                    return;
                }

                Task completed = pendingSend;
                pendingSend = null;
                ReleasePendingBuffer();

                if (completed.IsFaulted)
                {
                    Exception error = completed.Exception != null ? completed.Exception.GetBaseException() : null;
                    Debug.LogWarning("[CYG] send failed: " + (error != null ? error.Message : "unknown"));
                }

                if (socket.State != WebSocketState.Open)
                {
                    ClearOutbound();
                    RaiseDisconnected("socket closed while sending");
                    return;
                }
            }

            ArraySegment<byte> frame = outbound.Dequeue();
            pendingSendBuffer = frame.Array;
            try
            {
                pendingSend = socket.SendAsync(frame, WebSocketMessageType.Binary, true, ActiveToken);
            }
            catch (Exception error)
            {
                pendingSend = null;
                ReleasePendingBuffer();
                Debug.LogError("[CYG] send could not be scheduled: " + error.Message);
            }
        }

        CancellationToken ActiveToken
        {
            get
            {
                CancellationTokenSource source = cancellation;
                return source != null ? source.Token : CancellationToken.None;
            }
        }

        void ReleasePendingBuffer()
        {
            if (pendingSendBuffer != null)
            {
                ArrayPool<byte>.Shared.Return(pendingSendBuffer);
                pendingSendBuffer = null;
            }
        }

        void ClearOutbound()
        {
            while (outbound.Count > 0)
            {
                ArraySegment<byte> pending = outbound.Dequeue();
                if (pending.Array != null && !ReferenceEquals(pending.Array, pendingSendBuffer))
                {
                    ArrayPool<byte>.Shared.Return(pending.Array);
                }
            }

            ReleasePendingBuffer();
        }

        void DisposeSocket()
        {
            if (socket == null)
            {
                return;
            }

            try
            {
                if (socket.State == WebSocketState.Open || socket.State == WebSocketState.CloseReceived)
                {
                    socket.Abort();
                }

                socket.Dispose();
            }
            catch (Exception error)
            {
                Debug.LogWarning("[CYG] socket disposal failed: " + error.Message);
            }

            socket = null;
        }

        async void StartConnection(string url)
        {
            ClientWebSocket target = socket;
            try
            {
                await target.ConnectAsync(new Uri(url), ActiveToken);
            }
            catch (OperationCanceledException)
            {
                return;
            }
            catch (Exception error)
            {
                Debug.LogError("[CYG] could not reach the room server at " + url + ": " + error.Message);
                DisposeSocket();
                RaiseDisconnected(error.Message);
                return;
            }

            IsConnected = true;
            RaiseConnected();
            await ReceiveLoop(target);
        }

        async Task ReceiveLoop(ClientWebSocket target)
        {
            byte[] buffer = new byte[ReceiveChunkSize];
            string reason = "connection closed";

            while (target.State == WebSocketState.Open)
            {
                try
                {
                    WebSocketReceiveResult result = await target.ReceiveAsync(new ArraySegment<byte>(buffer), ActiveToken);
                    if (result.MessageType == WebSocketMessageType.Close)
                    {
                        reason = result.CloseStatusDescription ?? "remote close";
                        break;
                    }

                    AppendReceived(new ArraySegment<byte>(buffer, 0, result.Count), result.EndOfMessage);
                }
                catch (OperationCanceledException)
                {
                    return;
                }
                catch (WebSocketException error)
                {
                    reason = error.Message;
                    break;
                }
                catch (Exception error)
                {
                    reason = error.Message;
                    Debug.LogError("[CYG] receive loop failed: " + error.Message);
                    break;
                }
            }

            ResetReceiveAccumulator();
            // Only tear down the socket this loop owned: a reconnect may already have replaced it.
            if (ReferenceEquals(socket, target))
            {
                DisposeSocket();
            }

            RaiseDisconnected(reason);
        }
    }
}
