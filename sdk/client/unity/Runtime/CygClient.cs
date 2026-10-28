// CYG Unity SDK — same surface as the web SDK (openoxide.js), over raw UDP.
//
// Flow: matchmake (HTTP) → seat token → UDP join → room binds → deltas in,
// inputs out. State decode pairs with `ox-game schema-codegen --lang cs`
// output (CYGSchemaGenerated) via CygDeltaDecoder.
//
// Wire (server: crates/engine-server/src/plane/udp.rs):
//   JOIN  [0x20][tokenLen u8][seatToken utf8]
//   PING  [0x22][8B opaque]              → PONG [0x23][same 8B]
//   INPUT [op][seq u16 LE][20B payload]  (fixed 23B — the movement lane)
//   BIND  [0x24]…                        → server: you are bound to a room
// Deltas arrive as TLV rows — decode with CygDeltaDecoder onto generated state.

using System;
using System.Collections.Concurrent;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;

namespace CYG
{
    public sealed class CygClient : IDisposable
    {
        private UdpSocketHolder _sock;
        private string _seatToken;
        private string _room;
        private IPEndPoint _server;
        private ushort _inputSeq;
        private byte _pingCookie;
        private bool _running;
        private readonly ConcurrentQueue<byte[]> _deltas = new ConcurrentQueue<byte[]>();

        /// Player id the host assigned at bind (0 until then).
        public long PlayerId { get; private set; }
        public bool Connected { get; private set; }
        public string SessionId { get; private set; }

        /// Raw delta rows — feed to CygDeltaDecoder or your generated state.
        public event Action<byte[]> OnRawDelta;
        /// App-level messages (typed). Reserved for the OP_APP lane.
        public event Action<string, byte[]> OnMessage;
        public event Action<string> OnError;
        public event Action OnLeave;

        private CygClient() { }

        // ------------------------------------------------------------ join

        /// <summary>joinOrCreate equivalent: matchmake over HTTP, then UDP join.</summary>
        public static async Task<CygClient> JoinOrCreate(
            string httpOrigin, string roomName, string playerName = null)
        {
            var seat = await Matchmake(httpOrigin, "joinOrCreate", roomName, playerName);
            var client = new CygClient();
            await client.ConnectUdp(seat, roomName);
            return client;
        }

        private sealed class Seat { public string Host; public int Port; public string SeatToken; public string SessionId; }

        private static async Task<Seat> Matchmake(string origin, string method, string room, string playerName)
        {
            using var http = new System.Net.Http.HttpClient { Timeout = TimeSpan.FromSeconds(8) };
            var body = playerName == null ? "{}" : "{\"playerName\":\"" + playerName + "\"}";
            var res = await http.PostAsync($"{origin}/api/matchmake/{method}/{Uri.EscapeDataString(room)}",
                new System.Net.Http.StringContent(body, Encoding.UTF8, "application/json"));
            var json = await res.Content.ReadAsStringAsync();
            if (!res.IsSuccessStatusCode)
                throw new Exception($"matchmake refused: {res.StatusCode} {json}");
            return ParseSeat(json);
        }

        private static Seat ParseSeat(string json)
        {
            var seat = new Seat();
            seat.SeatToken = Extract(json, "seatToken") ?? Extract(json, "token");
            seat.SessionId = Extract(json, "sessionId") ?? "";
            var port = Extract(json, "gamePort") ?? Extract(json, "port") ?? "7777";
            var host = Extract(json, "host") ?? Extract(json, "address") ?? Extract(json, "ip");
            if (seat.SeatToken == null || host == null)
                throw new Exception($"seat missing token/host: {json}");
            seat.Host = host; seat.Port = int.Parse(port);
            return seat;
        }

        private static string Extract(string json, string key)
        {
            int i = json.IndexOf($"\"{key}\"", StringComparison.Ordinal);
            if (i < 0) return null;
            int colon = json.IndexOf(':', i + key.Length + 2);
            if (colon < 0) return null;
            int start = json.IndexOf('"', colon);
            if (start >= 0 && json.IndexOf(',', colon) > start || json.IndexOf('}', colon) > start)
            {
                int end = json.IndexOf('"', start + 1);
                return json.Substring(start + 1, end - start - 1);
            }
            int comma = json.IndexOfAny(new[] { ',', '}' }, colon);
            return json.Substring(colon + 1, comma - colon - 1).Trim();
        }

        // ------------------------------------------------------------ udp

        private async Task ConnectUdp(Seat seat, string room)
        {
            _seatToken = seat.SeatToken;
            _room = room;
            SessionId = seat.SessionId;
            _server = new IPEndPoint(IPAddress.Parse(seat.Host), seat.Port);
            _sock = new UdpSocketHolder();
            _sock.BindAndConnect(_server);

            // JOIN [0x20][tokenLen][token]
            var token = Encoding.UTF8.GetBytes(_seatToken);
            var join = new byte[2 + token.Length];
            join[0] = 0x20; join[1] = (byte)token.Length;
            Buffer.BlockCopy(token, 0, join, 2, token.Length);
            _sock.Send(join);

            _running = true;
            _ = Task.Run(ReceiveLoop);
            _ = Task.Run(PingLoop);

            // BIND arrives when the room has us; up to 5 s.
            for (int i = 0; i < 50 && !Connected; i++) await Task.Delay(100);
            if (!Connected) throw new Exception("UDP join timed out — no BIND from server");
        }

        private void ReceiveLoop()
        {
            var buf = new byte[65536];
            while (_running)
            {
                int n;
                try { n = _sock.Receive(buf); }
                catch (SocketException) { Thread.Sleep(2); continue; }
                catch (ObjectDisposedException) { break; }
                if (n <= 0) continue;
                HandlePacket(buf, n);
            }
        }

        private void HandlePacket(byte[] buf, int n)
        {
            switch (buf[0])
            {
                case 0x24: // BIND
                    Connected = true;
                    if (n >= 9) PlayerId = BitConverter.ToInt64(buf, 1);
                    break;
                case 0x23: break; // PONG — rtt bookkeeping lives with ping loop
                case 0x11: case 0xA5: // state delta rows (TLV) — decoder's job
                    var delta = new byte[n];
                    Buffer.BlockCopy(buf, 0, delta, 0, n);
                    _deltas.Enqueue(delta);
                    OnRawDelta?.Invoke(delta);
                    break;
                case 0x05: // typed app message / RPC response
                    if (n >= 3)
                    {
                        int tLen = buf[2];
                        if (n >= 3 + tLen)
                        {
                            var type = Encoding.UTF8.GetString(buf, 3, tLen);
                            var payload = new byte[n - 3 - tLen];
                            Buffer.BlockCopy(buf, 3 + tLen, payload, 0, payload.Length);
                            if (type == "__res") ResolveRpc(buf);
                            OnMessage?.Invoke(type, payload);
                        }
                    }
                    break;
                default: break;
            }
        }

        private async Task PingLoop()
        {
            while (_running)
            {
                var ping = new byte[9];
                ping[0] = 0x22;
                new Random().NextBytes(ping.AsSpan(1, 8));
                _sock.Send(ping);
                await Task.Delay(3000);
            }
        }

        // -------------------------------------------------------- messaging

        /// Movement input, unreliable newest-wins lane: [op][seq u16 LE][20B].
        /// Byte layout matches the web SDK's sendUnreliable input frame.
        public void SendInput(byte[] payload20)
        {
            if (payload20.Length != 20) throw new ArgumentException("input payload is exactly 20 bytes");
            var pkt = new byte[23];
            pkt[0] = 0x21; // OP_INP
            pkt[1] = (byte)(_inputSeq & 0xFF);
            pkt[2] = (byte)(_inputSeq >> 8);
            Buffer.BlockCopy(payload20, 0, pkt, 3, 20);
            _inputSeq++;
            _sock.Send(pkt);
        }

        /// Typed app message (reliable-style): [0x05][typeLen][type][payload]
        /// over UDP — guest ka onMessage/onRequest isse milta hai.
        public void Send(string type, byte[] payload = null)
        {
            var typeB = Encoding.UTF8.GetBytes(type);
            var frame = new byte[3 + typeB.Length + (payload?.Length ?? 0)];
            frame[0] = 0x05; frame[1] = (byte)typeB.Length;
            Buffer.BlockCopy(typeB, 0, frame, 2, typeB.Length);
            if (payload != null) Buffer.BlockCopy(payload, 0, frame, 2 + typeB.Length, payload.Length);
            _sock.Send(frame);
            OnMessage?.Invoke(type, payload ?? Array.Empty<byte>());
        }

        private ushort _reqSeq;
        private readonly ConcurrentDictionary<ushort, TaskCompletionSource<byte[]>> _pendingRpc
            = new ConcurrentDictionary<ushort, TaskCompletionSource<byte[]>>();

        /// request() RPC — guest ke onRequest ka jawab await karta hai (5s default).
        public Task<byte[]> Request(string type, byte[] payload = null, int timeoutMs = 5000)
        {
            _reqSeq = (ushort)((_reqSeq + 1) & 0xFFFF);
            ushort rid = _reqSeq == 0 ? (ushort)1 : _reqSeq;
            var typeB = Encoding.UTF8.GetBytes(type);
            var reqB = Encoding.UTF8.GetBytes("__req");
            var frame = new byte[3 + 5 + 2 + typeB.Length + (payload?.Length ?? 0)];
            frame[0] = 0x05; frame[1] = 5;
            frame[2] = (byte)'_'; frame[3] = (byte)'_'; frame[4] = (byte)'r'; frame[5] = (byte)'e'; frame[6] = (byte)'q';
            frame[7] = (byte)(rid >> 8); frame[8] = (byte)(rid & 0xFF);
            frame[9] = (byte)typeB.Length;
            Buffer.BlockCopy(typeB, 0, frame, 10, typeB.Length);
            if (payload != null) Buffer.BlockCopy(payload, 0, frame, 10 + typeB.Length, payload.Length);
            var tcs = new TaskCompletionSource<byte[]>(TaskCreationOptions.RunContinuationsAsynchronously);
            _pendingRpc[rid] = tcs;
            _sock.Send(frame);
            Task.Run(async () => {
                await Task.Delay(timeoutMs);
                if (_pendingRpc.TryRemove(rid, out var t)) t.TrySetException(new TimeoutException($"request '{type}' timed out"));
            });
            return tcs.Task;
        }

        private void ResolveRpc(byte[] frame)
        {
            // __res: [0x05][5][__res][reqId u16][payload]
            if (frame.Length < 9) return;
            ushort rid = (ushort)((frame[7] << 8) | frame[8]);
            if (_pendingRpc.TryRemove(rid, out var t))
                t.TrySetResult(frame.Skip(9).ToArray());
        }

        /// Leave politely: stop the loops; the server's idle sweep cleans the seat.
        public void Leave()
        {
            _running = false;
            _sock?.Dispose();
            Connected = false;
            OnLeave?.Invoke();
        }

        public void Dispose() => Leave();
    }

    /// Tiny UDP wrapper keeping the socket off Unity's main thread.
    internal sealed class UdpSocketHolder : IDisposable
    {
        private Socket _s;
        private IPEndPoint _server;

        public void BindAndConnect(IPEndPoint server)
        {
            _server = server;
            _s = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp);
            _s.Bind(new IPEndPoint(IPAddress.Any, 0));
            _s.Connect(server);
        }

        public void Send(byte[] data) => _s?.Send(data);

        public int Receive(byte[] buf)
        {
            if (_s.Available == 0) throw new SocketException(10035); // WouldBlock
            return _s.Receive(buf);
        }

        public void Dispose() => _s?.Dispose();
    }
}
