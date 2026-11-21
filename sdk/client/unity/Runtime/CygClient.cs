// CYG Unity SDK — same surface as the web SDK (cyg.js), over UDP or WebSocket.
//
// Flow: matchmake (HTTP) → seat token → join → room binds → deltas in, inputs
// out. ALL schema decode goes through the cyg-core-rs native lib: the host's
// descriptor ("OXSD") frame builds a CygRoom (P/Invoke core), and every schema
// delta is applied with CygRoom.ApplyPayload + typed reads (GetF64/GetString/
// GetBool by field index). CygDeltaDecoder remains as a thin shim over that
// core for code that still speaks rows.
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
// Schema deltas decode in cyg-core-rs; the OnRawDelta event only mirrors bytes.

using System;
using System.Collections.Concurrent;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Linq;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Cyg;

namespace CYG
{
    public sealed class CygClient : IDisposable
    {
        private ICygTransport _link;
        private string _seatToken;
        private string _room;
        private string _origin;
        private string _reconnectionToken;
        private IPEndPoint _server;
        private ushort _inputSeq;
        private byte _pingCookie;
        private bool _running;
        private volatile bool _leaving;
        private readonly ConcurrentQueue<byte[]> _deltas = new ConcurrentQueue<byte[]>();

        // Reconnect state. A player who loses a bar of wifi should see the game
        // stutter, not a menu — so this is on by default and the game is not
        // asked about it.
        private Lane _lane = Lane.Auto;
        private int _reconnectAttempts;
        private bool _everConnected;

        /// Player id the host assigned at bind (0 until then).
        public long PlayerId { get; private set; }
        public bool Connected { get; private set; }
        public string SessionId { get; private set; }

        /// Raw delta bytes — mirror only; the decoded state lives in
        /// <see cref="Room"/> (cyg-core-rs). Feed to CygDeltaDecoder (a shim
        /// over the same core) for the legacy row shape.
        public event Action<byte[]> OnRawDelta;
        /// Fired once when the host's schema descriptor lands and the core
        /// room is built. Read state via <see cref="Room"/> after this.
        public event Action OnRoomReady;
        /// The decoded room state (cyg-core-rs native lib). Null until the
        /// host's descriptor frame arrives.
        public CygRoom Room { get; private set; }
        /// App-level messages (typed). Reserved for the OP_APP lane.
        public event Action<string, byte[]> OnMessage;
        /// Routed voice frame: speaker id + 15-byte header + Opus payload.
        public event Action<long, byte[]> OnVoice;
        public event Action<string> OnError;
        public event Action OnLeave;
        /// <summary>
        /// The seat was lost and has been recovered. State arrives again as a
        /// full snapshot, so a caller that rebuilds from deltas should also
        /// treat this as "resync everything".
        /// </summary>
        public event Action OnReconnect;
        /// <summary>Fired before each retry. `(attempt, delayMs)`.</summary>
        public event Action<int, int> OnReconnecting;

        /// <summary>
        /// Recover the seat when the link drops. On by default.
        /// </summary>
        /// <remarks>
        /// A dropped socket is the normal case on mobile — a lift, a tunnel, a
        /// screen that slept. Surfacing that to a player as an error dialog is
        /// the wrong trade for a game that can rebuild its state in one frame.
        /// </remarks>
        public bool AutoReconnect { get; set; } = true;

        /// <summary>Set to 0 to retry until <see cref="Leave"/> is called.</summary>
        public int MaxReconnectAttempts { get; set; } = 6;

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
            var client = new CygClient
            {
                _origin = httpOrigin,
                _room = roomName,
                _lane = lane,
                _reconnectionToken = seat.ReconnectionToken,
            };

            // Try the preferred lane, then the other one. A host that opened
            // only one of them should not turn into a failed join the player
            // has to understand — the client knows enough to pick the other.
            Exception first = null;
            foreach (var attempt in LaneOrder(lane, seat))
            {
                try
                {
                    await client.Connect(seat, roomName, attempt, httpOrigin);
                    client._lane = attempt;
                    client._everConnected = true;
                    return client;
                }
                catch (Exception error)
                {
                    first ??= error;
                }
            }

            client.Dispose();
            throw new Exception(
                $"could not join '{roomName}' on any open lane — {first?.Message}", first);
        }

        /// <summary>
        /// The lanes to try, in order. Auto means "whatever the seat offers",
        /// with WebSocket last because every host that has a game lane at all
        /// serves it.
        /// </summary>
        private static IEnumerable<Lane> LaneOrder(Lane lane, Seat seat)
        {
            if (lane == Lane.Udp) { yield return Lane.Udp; yield break; }
            if (lane == Lane.WebSocket) { yield return Lane.WebSocket; yield break; }

            if (seat.HasUdp && UdpAvailable) yield return Lane.Udp;
            if (seat.GamePort > 0) yield return Lane.WebSocket;
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
            /// <summary>Lets a new socket resume this seat instead of taking another.</summary>
            public string ReconnectionToken;
        }

        private static async Task<Seat> Matchmake(
            string origin, string method, string room, string playerName, string reconnectionToken = null)
        {
            using var http = new System.Net.Http.HttpClient { Timeout = TimeSpan.FromSeconds(8) };
            string body;
            if (reconnectionToken != null)
                // A resume names the seat it wants back. Without the token the
                // host would hand out a second seat and the player would appear
                // twice in the room.
                body = System.Text.Json.JsonSerializer.Serialize(new { reconnectionToken });
            else
                body = playerName == null ? "{}" : System.Text.Json.JsonSerializer.Serialize(new { playerName });
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
            // the WebRTC DataChannel lane, a different socket). Seat contract:
            // udpPort is always serialized and 0 means UDP is OFF, so only a
            // positive udpPort enables the UDP lane; otherwise fall back to the
            // raw `port` for deployments that expose UDP on it.
            var udpRaw = Extract(json, "udpPort");
            seat.HasUdp = int.TryParse(udpRaw, out var udpPort) && udpPort > 0;
            var port = seat.HasUdp ? udpRaw
                : Extract(json, "gamePort") ?? Extract(json, "port") ?? "7777";
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
            seat.ReconnectionToken = Extract(json, "reconnectionToken");
            return seat;
        }

        // TODO: replace this hand-rolled scanner with System.Text.Json
        // (JsonDocument.Parse) end-to-end — see ParseSeat. Kept for now to
        // avoid touching the Seat API surface mid-rename.
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
                // String value: scan honoring backslash escapes so an escaped
                // quote (\") inside the value no longer truncates it.
                int end = p + 1;
                while (end < json.Length)
                {
                    char c = json[end];
                    if (c == '\\') { end += 2; continue; }
                    if (c == '"') break;
                    end++;
                }
                if (end >= json.Length) return null;
                return UnescapeJsonString(json.Substring(p + 1, end - p - 1));
            }
            // Number / bool / null: ends at the next comma or closing brace.
            // (The old version searched for a quote ANYWHERE after the colon,
            // so `"udpPort":7778,"expiresAt":…` extracted "expiresAt".)
            int stop = p;
            while (stop < json.Length && json[stop] != ',' && json[stop] != '}') stop++;
            return json.Substring(p, stop - p).Trim();
        }

        private static string UnescapeJsonString(string raw)
        {
            if (raw.IndexOf('\\') < 0) return raw;
            var sb = new System.Text.StringBuilder(raw.Length);
            for (int i = 0; i < raw.Length; i++)
            {
                char c = raw[i];
                if (c != '\\' || i + 1 >= raw.Length) { sb.Append(c); continue; }
                char n = raw[++i];
                switch (n)
                {
                    case '"': sb.Append('"'); break;
                    case '\\': sb.Append('\\'); break;
                    case '/': sb.Append('/'); break;
                    case 'n': sb.Append('\n'); break;
                    case 'r': sb.Append('\r'); break;
                    case 't': sb.Append('\t'); break;
                    case 'b': sb.Append('\b'); break;
                    case 'f': sb.Append('\f'); break;
                    default: sb.Append(n); break; // \uXXXX left as-is (seat fields never carry it)
                }
            }
            return sb.ToString();
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
                ICygTransport link;
                int n;
                try
                {
                    link = _link;
                    if (link == null) break;
                    n = link.Receive(buf);
                }
                // WouldBlock is the normal "no frame yet" answer on both lanes,
                // not a failure — the loop yields rather than spinning.
                catch (SocketException) { Thread.Sleep(2); continue; }
                catch (ObjectDisposedException) { break; }
                catch (Exception) { break; }

                if (n <= 0) continue;
                HandlePacket(buf, n);
            }

            // Falling out of the loop with the client still meant to be running
            // is the drop. `Leave` sets `_leaving` first so a polite exit is not
            // mistaken for one.
            if (!_leaving) BeginReconnect();
        }

        // ---------------------------------------------------------- reconnect

        /// <summary>
        /// The link went away. Take the seat back, quietly.
        /// </summary>
        /// <remarks>
        /// Runs on the receive thread, so it never blocks: each attempt is its
        /// own task and a failed one schedules the next. The player sees the
        /// game resume, not a reason.
        /// </remarks>
        private void BeginReconnect()
        {
            if (_leaving || !AutoReconnect || !_everConnected) return;
            if (Connected) Connected = false;
            _ = Task.Run(ReconnectLoop);
        }

        private async Task ReconnectLoop()
        {
            while (!_leaving)
            {
                if (MaxReconnectAttempts > 0 && _reconnectAttempts >= MaxReconnectAttempts)
                {
                    OnError?.Invoke($"could not reconnect after {_reconnectAttempts} attempts");
                    return;
                }

                var attempt = ++_reconnectAttempts;
                // 0.5s, 1s, 2s, 4s, capped at 8s. Short enough that a tunnel
                // feels like a hitch, long enough that a dead host is not
                // hammered by every player at once.
                var delay = (int)Math.Min(500 * Math.Pow(2, attempt - 1), 8000);
                OnReconnecting?.Invoke(attempt, delay);
                await Task.Delay(delay);
                if (_leaving) return;

                try
                {
                    if (!await TryResume()) continue;

                    _reconnectAttempts = 0;
                    OnReconnect?.Invoke();
                    return;
                }
                catch (Exception error)
                {
                    // A resume the host refused outright will not succeed on a
                    // retry either — say so and stop rather than waiting out
                    // the whole backoff ladder first.
                    if (error is CygApiException api && api.IsRefused)
                    {
                        OnError?.Invoke($"session could not be resumed: {error.Message}");
                        return;
                    }
                }
            }
        }

        /// <summary>One resume attempt. True when the seat is live again.</summary>
        private async Task<bool> TryResume()
        {
            var seat = await Matchmake(_origin, "reconnect", _room, null, _reconnectionToken);
            _reconnectionToken = seat.ReconnectionToken ?? _reconnectionToken;
            SessionId = seat.SessionId ?? SessionId;

            // A resume can land on a different node, which means a different
            // port — so the lane is chosen again from the fresh seat.
            foreach (var attempt in LaneOrder(_lane, seat))
            {
                try
                {
                    await Connect(seat, _room, attempt, _origin);
                    _lane = attempt;
                    return true;
                }
                catch (Exception) { /* try the next lane */ }
            }
            return false;
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
                case 0x11: case 0xA5: // schema delta / descriptor frame
                    var delta = new byte[n];
                    Buffer.BlockCopy(buf, 0, delta, 0, n);
                    _deltas.Enqueue(delta);
                    // The host pushes the schema descriptor ("OXSD") right
                    // after JOIN_ROOM; the core room is built from it and
                    // every later schema delta decodes through it.
                    if (CygRoom.IsSchemaDescriptor(delta))
                    {
                        if (Room == null)
                        {
                            try
                            {
                                Room = new CygRoom(delta);
                                CygDeltaDecoder.Attach(Room);
                                OnRoomReady?.Invoke();
                            }
                            catch (Exception error)
                            {
                                OnError?.Invoke("bad schema descriptor: " + error.Message);
                            }
                        }
                    }
                    else if (Room != null)
                    {
                        Room.ApplyPayload(delta);
                    }
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

        /// <summary>
        /// Kill the link without leaving, to exercise recovery.
        /// </summary>
        /// <remarks>
        /// A test hook, not a game one. The only honest way to check that a
        /// drop is recoverable is to cause a real one — a simulated flag would
        /// pass against a client that never reconnects.
        /// </remarks>
        public void DropForTest()
        {
            try { _link?.Dispose(); } catch (Exception) { }
        }

        /// Leave politely: stop the loops; the server's idle sweep cleans the seat.
        public void Leave()
        {
            // Set first: the receive loop reads it on the way out to tell a
            // polite exit from a drop, and it would otherwise start a resume
            // for a seat the player just gave up.
            _leaving = true;
            _running = false;
            _link?.Dispose();
            _link = null;
            Connected = false;
            Room?.Dispose();
            Room = null;
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
            if (Volatile.Read(ref _disposed) != 0) return;
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
            // A disposed lane has to say so. Answering WouldBlock forever kept
            // the receive loop spinning on a socket that was never coming back,
            // so a dropped WebSocket never looked like a drop — the client just
            // stopped receiving and waited.
            if (Volatile.Read(ref _disposed) != 0)
                throw new ObjectDisposedException(nameof(WebSocketTransport));
            if (!_incoming.TryDequeue(out var frame))
                throw new SocketException(10035); // WouldBlock — same contract as UDP
            var count = Math.Min(buffer.Length, frame.Length);
            Buffer.BlockCopy(frame, 0, buffer, 0, count);
            return count;
        }

        public void Dispose()
        {
            // Idempotent: a drop disposes the lane and so does `Leave`, and the
            // second call used to throw from a disposed semaphore — which the
            // caller then saw as a send failure on a socket it had already
            // given up on.
            if (Interlocked.Exchange(ref _disposed, 1) != 0) return;
            try
            {
                if (_socket?.State == System.Net.WebSockets.WebSocketState.Open)
                    _socket.CloseAsync(
                        System.Net.WebSockets.WebSocketCloseStatus.NormalClosure,
                        "leave", CancellationToken.None).GetAwaiter().GetResult();
            }
            catch (Exception) { }
            try { _closed.Cancel(); } catch (Exception) { }
            _socket?.Dispose();
            // The token source and the send lock are deliberately not disposed:
            // a background receive or an in-flight send may still hold them, and
            // disposing here turned a clean shutdown into an exception on a
            // thread nobody was watching.
        }

        private int _disposed;
    }
}
