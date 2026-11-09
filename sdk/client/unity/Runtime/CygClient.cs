// CYG Unity SDK — same surface as the web SDK (cyg.js), over UDP or WebSocket.
//
// Flow: matchmake (HTTP) → seat token → join → room binds → deltas in, inputs
// out. State decode pairs with `cyg schema-codegen --lang cs` output
// (CYGSchemaGenerated) via CygDeltaDecoder.
//
// Two lanes carry the same frames, because a Unity project does not always ship
// to a platform where raw UDP is available: consoles and desktop have it, a
// WebGL build does not (the browser offers no UDP socket), and WeChat hands the
// game a socket it only lets you talk to over WebSocket. The host's `[transports]`
// section decides which lanes exist; the SDK picks one that is actually open.
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
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;

namespace CYG
{
    public sealed class CygClient : IDisposable
    {
        private ICygTransport _link;
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
        /// Routed voice frame: speaker id + 15-byte header + Opus payload.
        public event Action<long, byte[]> OnVoice;
        public event Action<string> OnError;
        public event Action OnLeave;
        public event Action OnReconnect;

        private CygClient() { }

        // ------------------------------------------------------------ join

        /// <summary>
        /// False in a WebGL build, where the browser gives the game no UDP
        /// socket. Read from the compile symbol rather than a runtime probe,
        /// because the probe would be a socket that throws for a reason the
        /// caller cannot act on.
        /// </summary>
        public static bool UdpAvailable =>
#if UNITY_WEBGL && !UNITY_EDITOR
            false;
#else
            true;
#endif

        /// <summary>Which lane to open. <see cref="Auto"/> reads the seat.</summary>
        public enum Lane
        {
            /// <summary>UDP when the seat offers it, otherwise WebSocket.</summary>
            Auto,
            /// <summary>Raw UDP. Absent on WebGL and on a host with `udp = false`.</summary>
            Udp,
            /// <summary>WebSocket. Present on every host that has `websocket = true`.</summary>
            WebSocket,
        }

        /// <summary>
        /// joinOrCreate equivalent: matchmake over HTTP, then join on a lane.
        /// </summary>
        /// <param name="lane">
        /// Leave this <see cref="Lane.Auto"/> unless the project knows which
        /// lanes the host opened. Auto prefers UDP because it drops a stale
        /// input instead of queueing it behind a lost packet, and falls back to
        /// WebSocket — which is the only lane a browser build can use.
        /// </param>
        public static async Task<CygClient> JoinOrCreate(
            string httpOrigin, string roomName, string playerName = null, Lane lane = Lane.Auto)
        {
            var seat = await Matchmake(httpOrigin, "joinOrCreate", roomName, playerName);
            var client = new CygClient();
            await client.Connect(seat, roomName, lane, httpOrigin);
            return client;
        }

        private sealed class Seat
        {
            public string Host;
            public int Port;
            /// <summary>The WebSocket/WebRTC lane's port. Not the control API port.</summary>
            public int GamePort;
            public string SeatToken;
            public string SessionId;
            /// <summary>Set when the seat names a WebSocket lane.</summary>
            public string WsUrl;
            /// <summary>True when the seat carried a UDP port at all.</summary>
            public bool HasUdp;
        }

        private static async Task<Seat> Matchmake(string origin, string method, string room, string playerName)
        {
            using var http = new System.Net.Http.HttpClient { Timeout = TimeSpan.FromSeconds(8) };
            var body = playerName == null ? "{}" : "{\"playerName\":\"" + playerName + "\"}";
            var res = await http.PostAsync($"{origin}/api/matchmake/{method}/{Uri.EscapeDataString(room)}",
                new System.Net.Http.StringContent(body, Encoding.UTF8, "application/json"));
            var json = await res.Content.ReadAsStringAsync();
            if (!res.IsSuccessStatusCode)
                throw new Exception($"matchmake refused: {res.StatusCode} {json}");
            var seat = ParseSeat(json);
            // The control plane's seat carries no host field: answer to the
            // matchmake origin's host.
            if (seat.Host == null)
                seat.Host = new Uri(origin).Host;
            return seat;
        }

        private static Seat ParseSeat(string json)
        {
            var seat = new Seat();
            seat.SeatToken = Extract(json, "seatToken") ?? Extract(json, "token");
            seat.SessionId = Extract(json, "sessionId") ?? "";
            // The native-UDP game lane is seat.udpPort — NOT gamePort (that is
            // the WebRTC DataChannel lane, a different socket). Fall back to
            // the raw `port` for deployments that expose UDP on it.
            var port = Extract(json, "udpPort") ?? Extract(json, "gamePort") ?? Extract(json, "port") ?? "7777";
            seat.HasUdp = Extract(json, "udpPort") != null;
            if (int.TryParse(Extract(json, "gamePort") ?? "", out var gamePort))
                seat.GamePort = gamePort;
            // host may be null here — Matchmake defaults it to the matchmake
            // origin's host (the seat JSON carries no address field).
            var host = Extract(json, "host") ?? Extract(json, "address") ?? Extract(json, "ip");
            if (seat.SeatToken == null)
                throw new Exception($"seat missing token: {json}");
            seat.Host = host; seat.Port = int.Parse(port);
            // A host with `udp = false` still answers matchmake; the seat then
            // carries no udpPort and the WebSocket lane is the one to open.
            seat.WsUrl = Extract(json, "wsUrl") ?? Extract(json, "websocketUrl");
            return seat;
        }

        private static string Extract(string json, string key)
        {
            int i = json.IndexOf($"\"{key}\"", StringComparison.Ordinal);
            if (i < 0) return null;
            int colon = json.IndexOf(':', i + key.Length + 2);
            if (colon < 0) return null;
            int p = colon + 1;
            while (p < json.Length && char.IsWhiteSpace(json[p])) p++;
            if (p >= json.Length) return null;
            if (json[p] == '"')
            {
                // String value: ends at the next quote (no escape handling —
                // the seat JSON's string fields never carry escapes).
                int end = json.IndexOf('"', p + 1);
                return end < 0 ? null : json.Substring(p + 1, end - p - 1);
            }
            // Number / bool / null: ends at the next comma or closing brace.
            // (The old version searched for a quote ANYWHERE after the colon,
            // so `"udpPort":7778,"expiresAt":…` extracted "expiresAt".)
            int stop = p;
            while (stop < json.Length && json[stop] != ',' && json[stop] != '}') stop++;
            return json.Substring(p, stop - p).Trim();
        }

        // -------------------------------------------------------- transports

        /// <summary>
        /// Open the chosen lane and wait for the room to bind this client.
        /// </summary>
        /// <remarks>
        /// Both lanes speak the same frames, so everything above this method —
        /// input, messages, voice, delta decode — is written once. Only the
        /// seat token handshake and the framing differ.
        /// </remarks>
        private async Task Connect(Seat seat, string room, Lane lane, string httpOrigin)
        {
            var chosen = lane;
            if (chosen == Lane.Auto)
            {
                // UDP is preferred for play: a dropped packet costs one input
                // sample rather than stalling the whole stream behind a retransmit.
                chosen = seat.HasUdp && UdpAvailable ? Lane.Udp : Lane.WebSocket;
            }

            if (chosen == Lane.Udp)
                await ConnectUdp(seat, room);
            else
                await ConnectWebSocket(seat, room, httpOrigin);
        }

        /// <summary>
        /// Open the WebSocket lane.
        ///
        /// This is the lane a browser build must use — WebGL offers no UDP
        /// socket at all — and the one a host with `udp = false` still serves.
        /// The URL is derived from the matchmake origin unless the seat names
        /// one, so a project behind a proxy does not have to hard-code it.
        /// </summary>
        private async Task ConnectWebSocket(Seat seat, string room, string httpOrigin)
        {
            _seatToken = seat.SeatToken;
            _room = room;
            SessionId = seat.SessionId;

            var url = seat.WsUrl;
            if (string.IsNullOrEmpty(url))
            {
                // The room-data lane is its own listener, and the seat names its
                // port as `gamePort`. Using the matchmake origin here would open
                // a socket against the control API and get a 404 for an upgrade.
                var origin = new Uri(httpOrigin);
                var scheme = origin.Scheme == "https" ? "wss" : "ws";
                var host = string.IsNullOrEmpty(seat.Host) ? origin.Host : seat.Host;
                var port = seat.GamePort > 0 ? seat.GamePort : origin.Port;
                url = $"{scheme}://{host}:{port}/{Uri.EscapeDataString(room)}";
            }

            var link = new WebSocketTransport();
            await link.Open(url, SessionId);
            _link = link;

            _running = true;
            _ = Task.Run(ReceiveLoop);
            _ = Task.Run(PingLoop);

            // BIND arrives when the room has us; up to 5 s, same as UDP.
            for (int i = 0; i < 50 && !Connected; i++) await Task.Delay(100);
            if (!Connected) throw new Exception($"WebSocket join timed out — no BIND from {url}");
        }

        // ------------------------------------------------------------ udp

        private async Task ConnectUdp(Seat seat, string room)
        {
            _seatToken = seat.SeatToken;
            _room = room;
            SessionId = seat.SessionId;
            _server = new IPEndPoint(IPAddress.Parse(seat.Host), seat.Port);
            var link = new UdpSocketHolder();
            link.BindAndConnect(_server);
            _link = link;

            // JOIN [0x20][tokenLen][token]
            var token = Encoding.UTF8.GetBytes(_seatToken);
            var join = new byte[2 + token.Length];
            join[0] = 0x20; join[1] = (byte)token.Length;
            Buffer.BlockCopy(token, 0, join, 2, token.Length);
            _link.Send(join);

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
                try { n = _link.Receive(buf); }
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
                case 0x0a: // CYG_WIRE_JOIN_ROOM — the legacy raw-UDP lane's
                           // handshake reply: [op][rt_len]["room:session"][6]["schema"][0x00].
                           // The room is joined at this point; a separate
                           // datagram carries the schema descriptor.
                    Connected = true;
                    if (n >= 2)
                    {
                        int rtLen = buf[1];
                        if (n >= 2 + rtLen)
                        {
                            SessionId = Encoding.UTF8.GetString(buf, 2, rtLen);
                            // The token embeds the raw player id:
                            // "<room>:p<id>_<ip>_<port>".
                            var token = SessionId;
                            int colon = token.IndexOf(':');
                            var session = colon >= 0 ? token.Substring(colon + 1) : token;
                            if (session.StartsWith("p") && session.Length > 1)
                            {
                                int underscore = session.IndexOf('_', 1);
                                if (underscore > 1
                                    && long.TryParse(session.Substring(1, underscore - 1), out var pid))
                                    PlayerId = pid;
                            }
                        }
                    }
                    break;
                case 0x24: // BIND (the plane-lane variant)
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
                case 0x56: // routed voice frame — speaker id is stamped by the host
                    if (n >= 2)
                    {
                        long speakerId = n >= 5
                            ? ((long)buf[1] << 24) | ((long)buf[2] << 16) | ((long)buf[3] << 8) | buf[4]
                            : 0L;
                        var voice = new byte[n];
                        Buffer.BlockCopy(buf, 0, voice, 0, n);
                        OnVoice?.Invoke(speakerId, voice);
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
                _link.Send(ping);
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
            _link.Send(pkt);
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
            _link.Send(frame);
            OnMessage?.Invoke(type, payload ?? Array.Empty<byte>());
        }

        /// Unreliable single message: [0x05][typeLen][type][payload] — newest-wins.
        public void SendUnreliable(string type, byte[] payload = null)
        {
            var typeB = Encoding.UTF8.GetBytes(type);
            var frame = new byte[3 + typeB.Length + (payload?.Length ?? 0)];
            frame[0] = 0x05; frame[1] = (byte)typeB.Length;
            Buffer.BlockCopy(typeB, 0, frame, 2, typeB.Length);
            if (payload != null) Buffer.BlockCopy(payload, 0, frame, 2 + typeB.Length, payload.Length);
            _link.Send(frame);
        }

        /// Input batching — N movement samples ek packet me (lossy networks pe 4x kam packets).
        /// Layout: [0x26][count u8][pad]([seq u16 LE][20B payload])×count — server sequential apply karta hai.
        /// Server ke MAX_BATCHED_INPUTS (64) se upar wala batch poora reject hota hai.
        public void SendInputBatch((ushort seq, byte[] payload)[] samples)
        {
            var pkt = new byte[3 + samples.Length * 22];
            pkt[0] = 0x26; pkt[1] = (byte)samples.Length;
            int off = 3;
            foreach (var (seq, payload) in samples)
            {
                pkt[off] = (byte)(seq & 0xFF); pkt[off + 1] = (byte)(seq >> 8);
                Buffer.BlockCopy(payload, 0, pkt, off + 2, 20);
                off += 22;
            }
            _link.Send(pkt);
        }

        // ------------------------------------------------------------ voice
        //
        // Relay-only voice over the same UDP lane. The SDK owns the wire
        // framing, team/squad routing and mute/deafen control; mic capture,
        // Opus encode/decode and playback stay with the game (Unity's
        // Microphone + your Opus codec). Routed frames arrive on OnVoice /
        // CygVoice.OnPacket.

        private CygVoice _voice;

        /// <summary>Attach (or reuse) the room's relay-only voice handle.</summary>
        public CygVoice AttachVoice() => _voice ??= new CygVoice(this);

        /// <summary>FNV-1a over UTF-8 — same vectors as the web / Godot SDKs.</summary>
        public static uint HashTeam(string team)
        {
            uint hash = 2166136261u;
            var label = team == null ? "" : team.Trim();
            foreach (var b in Encoding.UTF8.GetBytes(label))
            {
                hash ^= b;
                hash *= 16777619u;
            }
            return hash;
        }

        /// <summary>
        /// [0x56]['T'][hash u32 BE] — 0 means "leave team". Team labels are
        /// hashed so the wire stays fixed-width and routing is one compare.
        /// </summary>
        public static byte[] BuildVoiceTeamPacket(string teamId)
        {
            var label = teamId == null ? "" : teamId.Trim();
            uint hash = label.Length == 0 ? 0u : HashTeam(label);
            return new byte[]
            {
                0x56, 0x54,
                (byte)(hash >> 24), (byte)(hash >> 16), (byte)(hash >> 8), (byte)hash,
            };
        }

        /// <summary>[0x56]['M'|'D'][1/0] — mic mute / deafen control frame.</summary>
        public static byte[] BuildVoiceFlagPacket(byte kind, bool on)
            => new byte[] { 0x56, kind, on ? (byte)1 : (byte)0 };

        /// <summary>
        /// 15-byte voice header + Opus payload, byte-for-byte with the web SDK:
        /// [magic][sender u32 BE][seq u16 BE][timestamp u32 BE][mode][flags][len u16 BE].
        /// The host overwrites the sender slot when relaying.
        /// </summary>
        public static byte[] BuildVoicePacket(
            VoiceChannel mode, ushort seq, uint timestamp, byte flags, byte[] opusPayload)
        {
            opusPayload ??= Array.Empty<byte>();
            if (opusPayload.Length > ushort.MaxValue)
                throw new ArgumentException("Opus payload is at most 65535 bytes", nameof(opusPayload));

            var pkt = new byte[15 + opusPayload.Length];
            pkt[0] = 0x56;
            pkt[5] = (byte)(seq >> 8); pkt[6] = (byte)seq;
            pkt[7] = (byte)(timestamp >> 24); pkt[8] = (byte)(timestamp >> 16);
            pkt[9] = (byte)(timestamp >> 8); pkt[10] = (byte)timestamp;
            pkt[11] = (byte)mode;
            pkt[12] = flags;
            pkt[13] = (byte)(opusPayload.Length >> 8); pkt[14] = (byte)opusPayload.Length;
            Buffer.BlockCopy(opusPayload, 0, pkt, 15, opusPayload.Length);
            return pkt;
        }

        /// <summary>Set the squad/team used by <see cref="VoiceChannel.TeamOnly"/>.</summary>
        public void SetVoiceTeam(string teamId) => _link?.Send(BuildVoiceTeamPacket(teamId));

        /// <summary>Number overload — stringified, then hashed like the web SDK.</summary>
        public void SetVoiceTeam(int teamId) => SetVoiceTeam(teamId.ToString());

        /// <summary>Notify the host that the local microphone is muted.</summary>
        public void SetVoiceMuted(bool muted) => _link?.Send(BuildVoiceFlagPacket(0x4D, muted));

        /// <summary>Notify the host that the local player cannot hear anyone.</summary>
        public void SetVoiceDeafened(bool deafened) => _link?.Send(BuildVoiceFlagPacket(0x44, deafened));

        /// <summary>Stream one Opus frame on the given channel.</summary>
        public void SendVoice(
            VoiceChannel mode, ushort seq, uint timestamp, byte flags, byte[] opusPayload)
            => _link?.Send(BuildVoicePacket(mode, seq, timestamp, flags, opusPayload));

        /// Server-clock aligned time (BIND ke baad ka offset).
        public long ClockNow => DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() + _clockOffsetMs;        private long _clockOffsetMs;

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
            _link.Send(frame);
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
            _link?.Dispose();
            _link = null;
            Connected = false;
            OnLeave?.Invoke();
        }

        /// Storage — web SDK ke localStorage ka parity. Unity me PlayerPrefs,
        /// non-Unity (tests/dotnet) me in-memory dictionary.
        public static class Storage
        {
            private static readonly Dictionary<string, string> Backing = new Dictionary<string, string>();

            public static void Set(string key, string value)
            {
#if UNITY_5_3_OR_NEWER
                UnityEngine.PlayerPrefs.SetString("cyg_" + key, value);
#endif
                Backing[key] = value;
            }

            public static string Get(string key, string fallback = "")
            {
#if UNITY_5_3_OR_NEWER
                if (UnityEngine.PlayerPrefs.HasKey("cyg_" + key))
                    return UnityEngine.PlayerPrefs.GetString("cyg_" + key);
#endif
                return Backing.TryGetValue(key, out var v) ? v : fallback;
            }

            public static void Remove(string key)
            {
#if UNITY_5_3_OR_NEWER
                UnityEngine.PlayerPrefs.DeleteKey("cyg_" + key);
#endif
                Backing.Remove(key);
            }
        }

        public void Dispose() => Leave();
    }

    /// Tiny UDP wrapper keeping the socket off Unity's main thread.
    /// <summary>
    /// What the client needs from a lane: send a frame, receive a frame, close.
    ///
    /// Deliberately tiny. UDP and WebSocket agree on nothing else — one is a
    /// datagram you may never receive, the other a stream that arrives whole
    /// but can stall — so anything richer would leak one lane's assumptions
    /// into the other.
    /// </summary>
    internal interface ICygTransport : IDisposable
    {
        void Send(byte[] data);
        /// <summary>One frame. Throws when none is ready, so the loop can yield.</summary>
        int Receive(byte[] buffer);
    }

    internal sealed class UdpSocketHolder : ICygTransport
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

    /// <summary>
    /// The WebSocket lane.
    ///
    /// A WebSocket carries messages, not packets, so the framing differs from
    /// UDP: every frame the client sends is one message, and every message the
    /// host sends is one frame. That is exactly how the web SDK talks, which
    /// is why a browser build and a Unity build can share a room.
    ///
    /// `ClientWebSocket` is used rather than a Unity-specific API because it
    /// needs no main-thread pump: received frames are read on a background
    /// task and handed to the client through a queue, so a frame arriving
    /// during a long frame does not stall the game.
    /// </summary>
    internal sealed class WebSocketTransport : ICygTransport
    {
        private System.Net.WebSockets.ClientWebSocket _socket;
        private readonly ConcurrentQueue<byte[]> _incoming = new ConcurrentQueue<byte[]>();
        private readonly CancellationTokenSource _closed = new CancellationTokenSource();
        private readonly SemaphoreSlim _sendLock = new SemaphoreSlim(1, 1);

        /// <param name="sessionId">
        /// The id the matchmake reply called `sessionId`. The host keys its
        /// pending seat on it, so the upgrade has to carry it — a client that
        /// omits it upgrades into a room with no seat and waits for a BIND
        /// that never comes.
        /// </param>
        public async Task Open(string url, string sessionId)
        {
            _socket = new System.Net.WebSockets.ClientWebSocket();
            _socket.Options.KeepAliveInterval = TimeSpan.FromSeconds(20);
            if (!string.IsNullOrEmpty(sessionId))
            {
                // Query parameter, not a header: a browser WebSocket cannot set
                // an Authorization header, and the web SDK passes the same one.
                url += (url.Contains("?") ? "&" : "?") + "sessionId=" + Uri.EscapeDataString(sessionId);
            }
            await _socket.ConnectAsync(new Uri(url), _closed.Token);

            _ = Task.Run(async () =>
            {
                var buffer = new byte[64 * 1024];
                var assembled = new List<byte>(64 * 1024);
                try
                {
                    while (!_closed.IsCancellationRequested)
                    {
                        assembled.Clear();
                        System.Net.WebSockets.WebSocketReceiveResult result;
                        do
                        {
                            result = await _socket.ReceiveAsync(
                                new ArraySegment<byte>(buffer), _closed.Token);
                            if (result.MessageType == System.Net.WebSockets.WebSocketMessageType.Close) return;
                            // A frame larger than the buffer arrives in pieces;
                            // treating a piece as a whole frame would hand the
                            // decoder half a delta.
                            assembled.AddRange(new ArraySegment<byte>(buffer, 0, result.Count));
                        }
                        while (!result.EndOfMessage);

                        if (assembled.Count > 0)
                            _incoming.Enqueue(assembled.ToArray());
                    }
                }
                catch (OperationCanceledException) { }
                catch (Exception) { /* a closed socket ends the loop */ }
            });
        }

        public void Send(byte[] data)
        {
            // Serialised: two threads sending at once would interleave the
            // fragments of two messages and the host would read garbage.
            _sendLock.Wait();
            try
            {
                _socket.SendAsync(
                    new ArraySegment<byte>(data),
                    System.Net.WebSockets.WebSocketMessageType.Binary,
                    true,
                    _closed.Token).GetAwaiter().GetResult();
            }
            catch (Exception) { /* a send on a dying socket is not worth a throw */ }
            finally { _sendLock.Release(); }
        }

        public int Receive(byte[] buffer)
        {
            if (!_incoming.TryDequeue(out var frame))
                throw new SocketException(10035); // WouldBlock — same contract as UDP
            var count = Math.Min(buffer.Length, frame.Length);
            Buffer.BlockCopy(frame, 0, buffer, 0, count);
            return count;
        }

        public void Dispose()
        {
            try
            {
                if (_socket?.State == System.Net.WebSockets.WebSocketState.Open)
                    _socket.CloseAsync(
                        System.Net.WebSockets.WebSocketCloseStatus.NormalClosure,
                        "leave", CancellationToken.None).GetAwaiter().GetResult();
            }
            catch (Exception) { }
            _closed.Cancel();
            _socket?.Dispose();
            _closed.Dispose();
            _sendLock.Dispose();
        }
    }
}
