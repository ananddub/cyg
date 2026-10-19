using System;
using System.Buffers;
using System.Collections.Generic;
using System.Text;
using UnityEngine;
using UnityEngine.Networking;

namespace CookYourGame
{
    /// <summary>Static entry point of the CookYourGame client SDK.</summary>
    public static class CYGNetwork
    {
        const float PingIntervalSeconds = 5f;
        const int SeatRequestTimeoutSeconds = 10;
        const ushort InvalidPrefabId = ushort.MaxValue;
        const int MaxEventNameBytes = 96;
        const string SerializerId = "schema";
        const string EventPlayerJoin = "cyg_player_join";
        const string EventPlayerLeave = "cyg_player_leave";

        // shared-types CloseCode: a seat dropped with one of these can never be
        // resumed, so the resume loop must not retry it.
        const int CloseServerShutdown = 4001;
        const int CloseWithError = 4002;
        const int CloseFailedToReconnect = 4003;

        static readonly CYGBitReader reader = new CYGBitReader();
        static readonly CYGBitWriter writer = new CYGBitWriter(1024);
        static readonly Dictionary<int, CYGPlayer> players = new Dictionary<int, CYGPlayer>();
        static readonly byte[] pingBody = new byte[4];

        static CYGTransport transport;
        static CYGPlayer localPlayer;
        static CYGServerSettings settingsOverride;
        static CYGPrefabRegistry prefabsOverride;
        static UnityWebRequest seatRequest;

        static string seatSessionId;
        static string seatRoomId;
        static string seatProcessId;
        static string seatToken;
        static int seatGamePort;
        static string pendingNickname;
        static string pendingRoomName;

        static int viewIndex;
        static double lastPingAt;
        static double lastPingSentAt;
        static bool joinHandshakeComplete;

        /// <summary>Raised when the room handshake completed and the client may play.</summary>
        public static event Action OnConnectedEvent;

        /// <summary>Raised when the link to the room dropped, with the reason.</summary>
        public static event Action<string> OnDisconnectedEvent;

        /// <summary>
        /// Raised when the server rejected the connection, with the protocol code
        /// and message. A seat refused because the room is full arrives here as
        /// <c>(4002, "room is full")</c> right before <see cref="OnDisconnectedEvent"/>.
        /// </summary>
        public static event Action<int, string> OnErrorEvent;

        /// <summary>Raised with the payload of an authoritative room state message.</summary>
        public static event Action<ArraySegment<byte>> OnRoomStateReceived;

        /// <summary>Raised with a named room event and the bytes that follow the name.</summary>
        public static event Action<string, ArraySegment<byte>> OnRoomEvent;

        /// <summary>Raised with a view id and its sync payload.</summary>
        public static event Action<int, ArraySegment<byte>> OnViewDelta;

        /// <summary>Raised with a raw rpc payload: viewId, method hash, target and arguments.</summary>
        public static event Action<ArraySegment<byte>> OnRpcReceived;

        /// <summary>Raised after a remote spawn has been instantiated locally.</summary>
        public static event Action<int, ArraySegment<byte>> OnObjectSpawned;

        /// <summary>Raised after a remote despawn has torn a view down.</summary>
        public static event Action<int> OnObjectDespawned;

        /// <summary>Raised for opaque binary room payloads the SDK does not interpret.</summary>
        public static event Action<ArraySegment<byte>> OnRoomDataBytes;

        /// <summary>Raised with a rewind hit payload sent by a peer.</summary>
        public static event Action<ArraySegment<byte>> OnRewindHitReceived;

        /// <summary>
        /// Wire opcode for deterministic input tick datagrams. 18 is reserved
        /// for PING/PONG; using it here would turn every heartbeat into input.
        /// </summary>
        public const byte CYG_WIRE_INPUT_TICK = 19;

        /// <summary>
        /// Raised for a deterministic input tick, whether it arrived as a raw
        /// UDP datagram (opcode 19) or inside ROOM_DATA_BYTES over WebSocket.
        /// The payload includes the leading opcode byte.
        /// </summary>
        public static event Action<ArraySegment<byte>> OnInputTickReceived;

        /// <summary>Raised as soon as the socket opens, before the room handshake.</summary>
        public static event Action OnConnectedToServer;

        /// <summary>Raised once the room handshake completed, with the room name.</summary>
        public static event Action<string> OnJoinedRoom;

        /// <summary>True while the room socket is usable.</summary>
        public static bool IsConnected
        {
            get { return transport != null && transport.IsConnected; }
        }

        /// <summary>True while a matchmake or socket connection is in flight.</summary>
        public static bool IsConnecting
        {
            get { return seatRequest != null || (transport != null && !transport.IsConnected && !string.IsNullOrEmpty(seatToken)); }
        }

        /// <summary>Type name of the active transport.</summary>
        public static string TransportName
        {
            get { return transport != null ? transport.GetType().Name : "WebSocket"; }
        }

        /// <summary>The local participant, or null while disconnected.</summary>
        public static CYGPlayer LocalPlayer
        {
            get { return localPlayer; }
        }

        /// <summary>Short id of the local participant, 0 when absent.</summary>
        public static int LocalActorNumber
        {
            get { return localPlayer != null ? localPlayer.ActorNumber : 0; }
        }

        /// <summary>True when the local participant holds room authority.</summary>
        public static bool IsMasterClient
        {
            get { return localPlayer != null && localPlayer.IsMasterClient; }
        }

        /// <summary>Every participant currently known to this client.</summary>
        public static IReadOnlyDictionary<int, CYGPlayer> Players
        {
            get { return players; }
        }

        /// <summary>Round-trip time of the last heartbeat in milliseconds.</summary>
        public static float Latency { get; private set; }

        /// <summary>Protocol code of the last server rejection (0 when none).</summary>
        public static int LastErrorCode { get; private set; }

        /// <summary>Message of the last server rejection, empty when none.</summary>
        public static string LastErrorMessage { get; private set; }

        /// <summary>Room name of the active or pending seat reservation.</summary>
        public static string RoomName
        {
            get { return string.IsNullOrEmpty(seatRoomId) ? pendingRoomName : seatRoomId; }
        }

        /// <summary>Session id issued by the matchmaker for the active seat.</summary>
        public static string SessionId
        {
            get { return seatSessionId; }
        }

        /// <summary>Settings used by ConnectToRoom; falls back to the scene manager.</summary>
        public static CYGServerSettings CurrentSettings
        {
            get { return settingsOverride != null ? settingsOverride : CYGNetworkManager.CurrentSettings; }
            set { settingsOverride = value; }
        }

        /// <summary>Prefab registry used for spawns; falls back to the scene manager.</summary>
        public static CYGPrefabRegistry CurrentPrefabs
        {
            get { return prefabsOverride != null ? prefabsOverride : CYGNetworkManager.CurrentPrefabs; }
            set { prefabsOverride = value; }
        }

        /// <summary>Active transport instance, created on first use.</summary>
        public static CYGTransport Transport
        {
            get { return transport; }
        }

        /// <summary>Opens the socket directly on a room url, skipping seat reservation.</summary>
        public static void Connect(string websocketUrl, string playerName = null)
        {
            if (string.IsNullOrEmpty(websocketUrl))
            {
                Debug.LogError("[CYG] Connect requires a room url.");
                return;
            }

            if (!websocketUrl.StartsWith("ws://", StringComparison.OrdinalIgnoreCase)
                && !websocketUrl.StartsWith("wss://", StringComparison.OrdinalIgnoreCase))
            {
                Debug.LogError("[CYG] Connect expects a ws:// or wss:// room url, received: " + websocketUrl);
                return;
            }

            pendingNickname = string.IsNullOrEmpty(playerName) ? DefaultNickname() : playerName;
            pendingRoomName = CYGServerSettings.NormalizeRoomName(ReadRoomFromSocketUrl(websocketUrl));
            EnsureTransport();
            transport.Connect(websocketUrl, seatToken);
        }

        /// <summary>Alias of ConnectToRoom with an optional capacity hint for the matchmaker.</summary>
        public static void JoinOrCreateRoom(string roomName, int maxPlayers = 0, string playerName = null)
        {
            if (maxPlayers > 0)
            {
                CYGServerSettings settings = CurrentSettings;
                if (settings != null)
                {
                    settings.MaxPlayers = maxPlayers;
                }
            }

            ConnectToRoom(roomName, playerName);
        }

        /// <summary>Alias of SpawnPrefab matching the package's Instantiate naming.</summary>
        public static CYGView Instantiate(string prefabName, Vector3 position, Quaternion rotation)
        {
            return SpawnPrefab(prefabName, position, rotation);
        }

        /// <summary>Connects directly to an authoritative match using matchmaker coordinates.</summary>
        public static void ConnectToMatch(string matchId, string token, string host = "127.0.0.1", int port = 7777, string playerName = null)
        {
            if (IsConnected || seatRequest != null)
            {
                Debug.LogWarning("[CYG] already connected or connecting; ConnectToMatch ignored.");
                return;
            }

            pendingRoomName = matchId;
            pendingNickname = string.IsNullOrEmpty(playerName) ? DefaultNickname() : playerName;

            seatRoomId = matchId;
            seatSessionId = Guid.NewGuid().ToString("N");
            seatToken = token;
            seatProcessId = "proc_1";
            seatGamePort = port;

            string wsScheme = "ws://";
            // Match the server's upgrade grammar: /<processId>/<roomId>?sessionId=...
            // so the server-side client key is the same id used to derive the
            // input-hub actor, and the room is not the literal path "ws".
            string socketUrl = $"{wsScheme}{host}:{port}/{seatProcessId}/{Uri.EscapeDataString(matchId)}?sessionId={Uri.EscapeDataString(seatSessionId)}&token={Uri.EscapeDataString(token ?? string.Empty)}";

            EnsureTransport();
            transport.Connect(socketUrl, token);
        }

        /// <summary>Reserves a seat over HTTP, opens the socket and joins the room.</summary>
        public static void ConnectToRoom(string roomName, string playerName = null)
        {
            if (IsConnected || seatRequest != null)
            {
                Debug.LogWarning("[CYG] already connected or connecting; ConnectToRoom ignored.");
                return;
            }

            CYGServerSettings settings = CurrentSettings;
            if (settings == null)
            {
                Debug.LogError("[CYG] ConnectToRoom needs CYGServerSettings: assign them on CYGNetworkManager or set CYGNetwork.CurrentSettings.");
                return;
            }

            pendingRoomName = CYGServerSettings.NormalizeRoomName(roomName);
            pendingNickname = string.IsNullOrEmpty(playerName) ? DefaultNickname() : playerName;

            string url = settings.MatchmakeUrl(settings.MatchmakeMethod, pendingRoomName);
            if (string.IsNullOrEmpty(url))
            {
                return;
            }

            string body = BuildMatchmakeBody(settings, pendingNickname);
            try
            {
                BeginSeatRequest(url, body);
            }
            catch (Exception error)
            {
                seatRequest = null;
                Debug.LogError("[CYG] matchmake request could not start: " + error.Message);
            }
        }

        /// <summary>Closes the room link and forgets the roster.</summary>
        public static void Disconnect()
        {
            // Drop the resume cookie before closing so the deliberate leave below cannot arm a reconnect.
            CYGSessionCookie.Clear();

            if (transport != null && transport.IsConnected)
            {
                writer.Reset();
                SendFrame(CYGOpCode.LeaveRoom, writer.ToArraySegment());
                SendFrame(CYGOpCode.LeaveRoom, writer.ToArraySegment());
                transport.Close();
            }

            AbandonSeatRequest();
            ClearRoomState();
        }

        /// <summary>Drains the transport queue and drives the heartbeat; called once per frame.</summary>
        public static void PumpInbound()
        {
            if (transport == null)
            {
                return;
            }

            transport.Pump();

            if (!IsConnected)
            {
                return;
            }

            double now = Time.realtimeSinceStartupAsDouble;
            if (now - lastPingAt >= PingIntervalSeconds)
            {
                lastPingAt = now;
                lastPingSentAt = now;
                WriteUInt32BigEndian(pingBody, 0, (uint)(now * 1000.0));
                SendFrame(CYGOpCode.Ping, new ArraySegment<byte>(pingBody));
            }
        }

        /// <summary>Frames a payload as [len u16 BE][op][payload] and hands it to the transport.</summary>
        public static bool SendFrame(CYGOpCode op, ArraySegment<byte> framePayload)
        {
            if (transport == null || !transport.IsConnected)
            {
                Debug.LogWarning("[CYG] " + op + " was dropped: not connected.");
                return false;
            }

            if (framePayload.Count > ushort.MaxValue)
            {
                Debug.LogError("[CYG] payload of " + framePayload.Count + " bytes cannot fit the 16-bit length header.");
                return false;
            }

            int total = CYGWireProtocol.HeaderSize + framePayload.Count;
            byte[] buffer = ArrayPool<byte>.Shared.Rent(total);
            buffer[0] = (byte)((framePayload.Count >> 8) & 0xFF);
            buffer[1] = (byte)(framePayload.Count & 0xFF);
            buffer[2] = (byte)op;
            if (framePayload.Count > 0)
            {
                Buffer.BlockCopy(framePayload.Array, framePayload.Offset, buffer, CYGWireProtocol.HeaderSize, framePayload.Count);
            }

            transport.Send(new ArraySegment<byte>(buffer, 0, total));
            ArrayPool<byte>.Shared.Return(buffer);
            return true;
        }

        /// <summary>Sends a pre-encoded rpc payload: [viewId varint][methodHash u16 BE][target u8][args].</summary>
        public static bool SendRpcPayload(ArraySegment<byte> rpcPayload)
        {
            return SendFrame(CYGOpCode.CustomRpc, rpcPayload);
        }

        /// <summary>Sends a view sync delta as [varint viewId][payload].</summary>
        public static bool SendViewDelta(int viewId, ArraySegment<byte> deltaPayload)
        {
            writer.Reset();
            writer.WriteInt32(viewId);
            writer.WriteRaw(deltaPayload);
            return SendFrame(CYGOpCode.StateDelta, writer.ToArraySegment());
        }

        /// <summary>Sends a player input payload, opaque to the SDK.</summary>
        public static bool SendInputPayload(ArraySegment<byte> inputPayload)
        {
            return SendFrame(CYGOpCode.PlayerInput, inputPayload);
        }

        /// <summary>Sends a raw UDP datagram directly without transport wrapping, or frames if TCP.</summary>
        public static bool SendRaw(byte opcode, byte[] buffer)
        {
            if (buffer == null) return false;
            return SendRaw(opcode, new ArraySegment<byte>(buffer, 0, buffer.Length));
        }

        /// <summary>Sends a raw UDP datagram directly without transport wrapping, or frames if TCP.</summary>
        public static bool SendRaw(byte opcode, ArraySegment<byte> buffer)
        {
            if (transport == null || !transport.IsConnected)
            {
                return false;
            }

            if (transport is CYGUdpTransport udp)
            {
                if (buffer.Count > 0 && buffer.Array != null && buffer.Array[buffer.Offset] == opcode)
                {
                    udp.SendRaw(buffer);
                    return true;
                }

                int total = 1 + buffer.Count;
                byte[] raw = ArrayPool<byte>.Shared.Rent(total);
                try
                {
                    raw[0] = opcode;
                    if (buffer.Count > 0 && buffer.Array != null)
                    {
                        Buffer.BlockCopy(buffer.Array, buffer.Offset, raw, 1, buffer.Count);
                    }
                    udp.SendRaw(new ArraySegment<byte>(raw, 0, total));
                    return true;
                }
                finally
                {
                    ArrayPool<byte>.Shared.Return(raw);
                }
            }

            return SendFrame((CYGOpCode)opcode, buffer);
        }

        /// <summary>Sends a named room event as [msgpack str header][utf8 name][body].</summary>
        public static bool SendRoomEvent(string eventName, ArraySegment<byte> body)
        {
            if (string.IsNullOrEmpty(eventName))
            {
                Debug.LogError("[CYG] SendRoomEvent requires an event name.");
                return false;
            }

            writer.Reset();
            writer.WriteString(eventName);
            writer.WriteRaw(body);
            return SendFrame(CYGOpCode.ProtoRoomData, writer.ToArraySegment());
        }

        /// <summary>Instantiates a registered prefab locally and tells the room about it.</summary>
        public static CYGView SpawnPrefab(string prefabName, Vector3 position, Quaternion rotation)
        {
            if (string.IsNullOrEmpty(prefabName))
            {
                Debug.LogError("[CYG] SpawnPrefab requires a prefab name.");
                return null;
            }

            CYGPrefabRegistry registry = CurrentPrefabs;
            if (registry == null)
            {
                Debug.LogError("[CYG] SpawnPrefab needs a CYGPrefabRegistry: assign one on CYGNetworkManager.");
                return null;
            }

            ushort prefabId = registry.GetPrefabId(prefabName);
            if (prefabId == InvalidPrefabId)
            {
                return null;
            }

            GameObject prefab = registry.GetPrefab(prefabId);
            if (prefab == null)
            {
                return null;
            }

            int ownerActor = LocalActorNumber;
            int viewId = NextViewIdForOwner(ownerActor);

            CYGView view = AcquireInstance(prefab, position, rotation);
            view.AssignViewId(viewId, ownerActor);

            writer.Reset();
            writer.WriteInt32(viewId);
            writer.WriteUInt16BigEndian(prefabId);
            writer.WriteUInt32((uint)ownerActor);
            writer.WriteVector3(position);
            writer.WriteQuaternionRaw(rotation);
            SendFrame(CYGOpCode.SpawnObject, writer.ToArraySegment());

            view.InvokeSpawnHooks();
            return view;
        }

        /// <summary>Tears a view down locally and tells the room about it.</summary>
        public static void DestroyObject(CYGView view)
        {
            if (view == null)
            {
                Debug.LogError("[CYG] DestroyObject received a null view.");
                return;
            }

            int viewId = view.ViewId;
            if (viewId != 0)
            {
                writer.Reset();
                writer.WriteInt32(viewId);
                SendFrame(CYGOpCode.DespawnObject, writer.ToArraySegment());
            }

            ReleaseInstance(view);
        }

        /// <summary>Hands out a pooled instance when the pool knows this prefab, otherwise a fresh clone. Identity is left unassigned.</summary>
        static CYGView AcquireInstance(GameObject prefab, Vector3 position, Quaternion rotation)
        {
            CYGView pooled = CYGObjectPool.SpawnView(prefab, position, rotation);
            if (pooled != null)
            {
                return pooled;
            }

            GameObject instance = UnityEngine.Object.Instantiate(prefab, position, rotation);
            CYGView view = instance.GetComponent<CYGView>();
            if (view == null)
            {
                view = instance.AddComponent<CYGView>();
            }

            return view;
        }

        /// <summary>Returns a view's GameObject to the pool, or destroys it when the pool never owned it. Fires despawn hooks exactly once.</summary>
        static void ReleaseInstance(CYGView view)
        {
            if (CYGObjectPool.Despawn(view))
            {
                return;
            }

            view.InvokeDespawnHooks();
            if (view.gameObject != null)
            {
                UnityEngine.Object.Destroy(view.gameObject);
            }
        }

        /// <summary>Allocates a view id owned by an actor: (actor << 16) | counter.</summary>
        public static int NextViewIdForOwner(int actorNumber)
        {
            if (actorNumber < 0)
            {
                actorNumber = 0;
            }

            viewIndex = (viewIndex + 1) & 0xFFFF;
            return (actorNumber << 16) | viewIndex;
        }

        static void BeginSeatRequest(string url, string jsonBody)
        {
            AbandonSeatRequest();

            seatRequest = new UnityWebRequest(url, UnityWebRequest.kHttpVerbPOST);
            byte[] raw = Encoding.UTF8.GetBytes(jsonBody);
            seatRequest.uploadHandler = new UploadHandlerRaw(raw);
            seatRequest.downloadHandler = new DownloadHandlerBuffer();
            seatRequest.SetRequestHeader("Content-Type", "application/json");
            seatRequest.timeout = SeatRequestTimeoutSeconds;

            UnityWebRequest request = seatRequest;
            request.SendWebRequest().completed += _ =>
            {
                if (!ReferenceEquals(request, seatRequest))
                {
                    request.Dispose();
                    return;
                }

                try
                {
                    OnSeatResponse(request);
                }
                finally
                {
                    seatRequest = null;
                    request.Dispose();
                }
            };
        }

        static void OnSeatResponse(UnityWebRequest request)
        {
            if (request.result != UnityWebRequest.Result.Success)
            {
                string detail = request.downloadHandler != null ? request.downloadHandler.text : string.Empty;
                Debug.LogError("[CYG] matchmake failed (" + request.responseCode + "): " + detail);
                ClearRoomState();
                if (OnDisconnectedEvent != null)
                {
                    OnDisconnectedEvent.Invoke("matchmake failed: " + detail);
                }

                return;
            }

            string json = request.downloadHandler.text;
            seatSessionId = ReadJsonString(json, "sessionId");
            seatRoomId = ReadJsonString(json, "roomId");
            seatProcessId = ReadJsonString(json, "processId");
            seatToken = ReadJsonString(json, "reconnectionToken");
            if (string.IsNullOrEmpty(seatToken))
            {
                seatToken = ReadJsonString(json, "seatToken");
            }

            seatGamePort = ReadJsonInt(json, "gamePort");
            int udpPort = ReadJsonInt(json, "udpPort");

            if (string.IsNullOrEmpty(seatSessionId))
            {
                Debug.LogError("[CYG] matchmake reply carried no sessionId: " + json);
                return;
            }

            CYGSessionCookie.Capture(seatRoomId, seatSessionId, seatToken, seatProcessId, seatGamePort);

            CYGServerSettings settings = CurrentSettings;
            if (settings == null)
            {
                Debug.LogError("[CYG] seat received but no CYGServerSettings are available to build the socket url.");
                return;
            }

#if !UNITY_WEBGL
            if (udpPort > 0)
            {
                string udpEndpoint = settings.UdpEndpoint(udpPort);
                Debug.Log($"[CYG] Matchmaker provided UDP port {udpPort}; connecting via zero-jitter CYGUdpTransport to {udpEndpoint}");
                EnsureTransport(preferUdp: true);
                transport.Connect(udpEndpoint, seatToken);
                return;
            }
#endif

            string socketUrl = settings.GameSocketUrl(seatProcessId, seatRoomId, seatSessionId, seatToken, seatGamePort);
            if (string.IsNullOrEmpty(socketUrl))
            {
                return;
            }

            EnsureTransport(preferUdp: false);
            transport.Connect(socketUrl, seatToken);
        }

        /// <summary>Optional replacement for the default WebSocket transport, consulted the first time a link is needed
        /// (e.g. <c>CYGNetwork.TransportFactory = () =&gt; new CYGProfilingTransport(new CYGWebSocketTransport());</c>).</summary>
        public static Func<CYGTransport> TransportFactory;

        static void EnsureTransport(bool preferUdp = false)
        {
            if (transport != null)
            {
#if !UNITY_WEBGL
                if (preferUdp && !(transport is CYGUdpTransport))
                {
                    transport.Close();
                    transport = null;
                }
                else if (!preferUdp && !(transport is CYGWebSocketTransport) && TransportFactory == null)
                {
                    transport.Close();
                    transport = null;
                }
                else
                {
                    return;
                }
#else
                return;
#endif
            }

            if (TransportFactory != null)
            {
                transport = TransportFactory();
            }
#if !UNITY_WEBGL
            else if (preferUdp)
            {
                transport = new CYGUdpTransport();
            }
#endif
            else
            {
                transport = new CYGWebSocketTransport();
            }

            transport.OnConnected += HandleSocketOpen;
            transport.OnDisconnected += HandleSocketClosed;
            transport.OnFrame += HandleFrame;
            transport.OnError += HandleServerError;
        }

        static void HandleSocketOpen()
        {
            joinHandshakeComplete = false;
            // A fresh link starts with a clean error surface: a rejection for
            // this attempt can only arrive after the socket is open.
            LastErrorCode = 0;
            LastErrorMessage = string.Empty;
            lastPingAt = Time.realtimeSinceStartupAsDouble;
            lastPingSentAt = lastPingAt;

            writer.Reset();
            WriteJoinHandshakeBody(writer, seatToken);
            SendFrame(CYGOpCode.ProtoJoinRoom, writer.ToArraySegment());

            if (OnConnectedToServer != null)
            {
                OnConnectedToServer.Invoke();
            }
        }

        static void HandleSocketClosed(string reason)
        {
            int closeCode = transport != null ? transport.LastCloseCode : 0;
            if (IsTerminalClose(closeCode) || IsTerminalClose(LastErrorCode))
            {
                // The server refused or ended the seat (full room, shutting-down
                // node, a resume it will not honour). Re-presenting the same
                // reconnection token can only fail the same way, so stop the
                // resume loop instead of backing off and retrying it. Either
                // signal is enough: the close frame can be missed when the drop
                // is first noticed by the send path, and the ERROR message can be
                // missed when the server closes without one.
                if (closeCode == CloseWithError && LastErrorCode == 0)
                {
                    // A close with no preceding wire ERROR still deserves a reason.
                    ReportError(closeCode, reason);
                }

                CYGSessionCookie.Clear();
            }
            else
            {
                CYGSessionCookie.NotifyDisconnected(reason);
            }

            ClearRoomState();
            if (OnDisconnectedEvent != null)
            {
                OnDisconnectedEvent.Invoke(reason);
            }
        }

        /// <summary>
        /// True when a code means the seat can never be resumed: the node is
        /// shutting down, it refused the join, or it gave up restoring the seat.
        /// </summary>
        static bool IsTerminalClose(int code)
        {
            return code == CloseServerShutdown
                || code == CloseWithError
                || code == CloseFailedToReconnect;
        }

        /// <summary>
        /// Turns a wire ERROR (11) payload into the public error surface. The
        /// engine sends JSON (<c>{"code":4002,"message":"room is full"}</c>); any
        /// other shape is passed through as a plain message.
        /// </summary>
        static void HandleServerError(string payload)
        {
            string detail = payload ?? string.Empty;
            string message = ReadJsonString(detail, "message");
            ReportError(ReadJsonInt(detail, "code"), string.IsNullOrEmpty(message) ? detail : message);
        }

        static void ReportError(int code, string message)
        {
            LastErrorCode = code;
            LastErrorMessage = message ?? string.Empty;
            Debug.LogError("[CYG] the room server rejected the connection (" + code + "): " + LastErrorMessage);

            if (OnErrorEvent != null)
            {
                OnErrorEvent.Invoke(code, LastErrorMessage);
            }
        }

        static void HandleFrame(ArraySegment<byte> frame)
        {
            if (frame.Array == null || frame.Count < CYGWireProtocol.HeaderSize)
            {
                Debug.LogWarning("[CYG] received a runt frame of " + frame.Count + " bytes.");
                return;
            }

            byte[] data = frame.Array;
            int origin = frame.Offset;
            int payloadLength = (data[origin] << 8) | data[origin + 1];
            if (CYGWireProtocol.HeaderSize + payloadLength > frame.Count)
            {
                Debug.LogWarning("[CYG] frame header claims " + payloadLength + " payload bytes but only " + (frame.Count - CYGWireProtocol.HeaderSize) + " arrived.");
                return;
            }

            CYGOpCode op = (CYGOpCode)data[origin + 2];
            ArraySegment<byte> payload = new ArraySegment<byte>(data, origin + CYGWireProtocol.HeaderSize, payloadLength);

            try
            {
                Route(op, payload);
            }
            catch (Exception error)
            {
                Debug.LogWarning("[CYG] could not decode a " + op + " frame: " + error.Message);
            }
        }

        static void Route(CYGOpCode op, ArraySegment<byte> payload)
        {
            switch (op)
            {
                case CYGOpCode.Pong:
                    Latency = (float)((Time.realtimeSinceStartupAsDouble - lastPingSentAt) * 1000.0);
                    break;

                case CYGOpCode.Ping:
                    break;

                case CYGOpCode.ProtoJoinRoom:
                    HandleJoinHandshake(payload);
                    break;

                case CYGOpCode.ProtoRoomState:
                case CYGOpCode.StateSnapshot:
                    if (OnRoomStateReceived != null)
                    {
                        OnRoomStateReceived.Invoke(payload);
                    }

                    break;

                case CYGOpCode.ProtoRoomData:
                    HandleRoomData(payload);
                    break;

                case CYGOpCode.ProtoStatePatch:
                case CYGOpCode.StateDelta:
                    HandleViewDelta(payload);
                    break;

                case CYGOpCode.CustomRpc:
                    if (OnRpcReceived != null)
                    {
                        OnRpcReceived.Invoke(payload);
                    }

                    break;

                case CYGOpCode.SpawnObject:
                    HandleSpawn(payload);
                    break;

                case CYGOpCode.DespawnObject:
                    HandleDespawn(payload);
                    break;

                case CYGOpCode.RewindHit:
                    if (OnRewindHitReceived != null)
                    {
                        OnRewindHitReceived.Invoke(payload);
                    }

                    break;

                case CYGOpCode.ProtoDataBytes:
                    HandleRoomBytes(payload);
                    break;

                case CYGOpCode.InputTick:
                    if (OnInputTickReceived != null)
                    {
                        OnInputTickReceived.Invoke(payload);
                    }

                    break;

                case CYGOpCode.ProtoLeaveRoom:
                    // The room server answers a leave with a socket close, so this
                    // frame only ever arrives from a CYG-aware host.
                    if (OnRoomDataBytes != null)
                    {
                        OnRoomDataBytes.Invoke(payload);
                    }

                    break;

                default:
                    Debug.LogWarning("[CYG] no route for opcode " + op + ".");
                    break;
            }
        }

        static void HandleJoinHandshake(ArraySegment<byte> payload)
        {
            reader.Attach(payload.Array, payload.Offset, payload.Count);

            string reconnectionToken = string.Empty;
            string serializer = string.Empty;
            byte tokenLength = reader.Remaining > 0 ? reader.ReadByte() : (byte)0;
            if (tokenLength > 0 && reader.Remaining >= tokenLength)
            {
                ArraySegment<byte> token = reader.ReadBytes(tokenLength);
                reconnectionToken = Encoding.UTF8.GetString(token.Array, token.Offset, token.Count);
            }

            byte serializerLength = reader.Remaining > 0 ? reader.ReadByte() : (byte)0;
            if (serializerLength > 0 && reader.Remaining >= serializerLength)
            {
                ArraySegment<byte> serializerBytes = reader.ReadBytes(serializerLength);
                serializer = Encoding.UTF8.GetString(serializerBytes.Array, serializerBytes.Offset, serializerBytes.Count);
            }

            if (!string.IsNullOrEmpty(reconnectionToken) && reconnectionToken.IndexOf(':') >= 0)
            {
                int firstColon = reconnectionToken.IndexOf(':');
                seatRoomId = reconnectionToken.Substring(0, firstColon);
                seatSessionId = reconnectionToken.Substring(firstColon + 1);
                CYGSessionCookie.Capture(seatRoomId, seatSessionId, reconnectionToken);
            }

            if (serializer.Length > 0 && serializer != SerializerId)
            {
                Debug.LogWarning("[CYG] room serializer \"" + serializer + "\" is not the schema codec this SDK decodes.");
            }

            RegisterLocalPlayer();
            joinHandshakeComplete = true;

            if (OnConnectedEvent != null)
            {
                OnConnectedEvent.Invoke();
            }

            if (OnJoinedRoom != null)
            {
                OnJoinedRoom.Invoke(RoomName);
            }
        }

        static void HandleRoomData(ArraySegment<byte> payload)
        {
            string eventName;
            ArraySegment<byte> body;
            if (!TrySplitEventName(payload, out eventName, out body))
            {
                eventName = string.Empty;
                body = payload;
            }

            TrackRosterEvent(eventName, body);

            if (OnRoomEvent != null)
            {
                OnRoomEvent.Invoke(eventName, body);
            }
        }

        static void HandleRoomBytes(ArraySegment<byte> payload)
        {
            // WebSocket peers receive the input hub as ROOM_DATA_BYTES. Its
            // first byte is the nested input-tick opcode; surface it through
            // the same event as raw UDP so mixed WS/UDP rosters work.
            if (payload.Count >= 16
                && payload.Array != null
                && payload.Array[payload.Offset] == CYG_WIRE_INPUT_TICK)
            {
                if (OnInputTickReceived != null)
                {
                    OnInputTickReceived.Invoke(payload);
                }

                return;
            }

            string eventName;
            ArraySegment<byte> body;
            if (TrySplitEventName(payload, out eventName, out body))
            {
                TrackRosterEvent(eventName, body);
                if (OnRoomEvent != null)
                {
                    OnRoomEvent.Invoke(eventName, body);
                }

                return;
            }

            if (OnRoomDataBytes != null)
            {
                OnRoomDataBytes.Invoke(payload);
            }
        }

        static void HandleViewDelta(ArraySegment<byte> payload)
        {
            reader.Attach(payload.Array, payload.Offset, payload.Count);
            int viewId = reader.ReadInt32();
            ArraySegment<byte> body = new ArraySegment<byte>(
                payload.Array,
                payload.Offset + reader.BytesRead,
                payload.Count - reader.BytesRead);

            // Behaviour components subscribe to OnViewDelta and decode the body themselves,
            // so the SDK does not touch the view table on this hot path.
            if (OnViewDelta != null)
            {
                OnViewDelta.Invoke(viewId, body);
            }
        }

        static void HandleSpawn(ArraySegment<byte> payload)
        {
            reader.Attach(payload.Array, payload.Offset, payload.Count);
            int viewId = reader.ReadInt32();
            ushort prefabId = reader.ReadUInt16BigEndian();
            int ownerActor = (int)reader.ReadUInt32();
            Vector3 position = reader.ReadVector3();
            Quaternion rotation = reader.ReadQuaternionRaw();

            // Guard against duplicate spawns: if viewId is already live, ignore!
            CYGView existing;
            if (CYGViewTable.TryGet(viewId, out existing) && existing != null)
            {
                return;
            }

            Debug.Log($"[CYG] Remote spawn arrived: viewId={viewId}, prefabId={prefabId}, ownerActor={ownerActor} (LocalActor={LocalActorNumber})");

            ArraySegment<byte> arguments = new ArraySegment<byte>(
                payload.Array,
                payload.Offset + reader.BytesRead,
                payload.Count - reader.BytesRead);

            CYGPrefabRegistry registry = CurrentPrefabs;
            GameObject prefab = registry != null ? registry.GetPrefab(prefabId) : null;
            if (prefab == null && CYGNetworkManager.Instance != null && CYGNetworkManager.Instance.PlayerPrefab != null)
            {
                prefab = CYGNetworkManager.Instance.PlayerPrefab;
            }
            if (prefab == null)
            {
                Debug.LogError("[CYG] remote spawn references unknown prefab id " + prefabId + ".");
                return;
            }

            CYGView view = AcquireInstance(prefab, position, rotation);
            view.AssignViewId(viewId, ownerActor);
            view.InvokeSpawnHooks();

            if (OnObjectSpawned != null)
            {
                OnObjectSpawned.Invoke(viewId, arguments);
            }
        }

        static void HandleDespawn(ArraySegment<byte> payload)
        {
            reader.Attach(payload.Array, payload.Offset, payload.Count);
            int viewId = reader.ReadInt32();

            CYGView view;
            if (CYGViewTable.TryGet(viewId, out view) && view != null)
            {
                ReleaseInstance(view);
            }

            if (OnObjectDespawned != null)
            {
                OnObjectDespawned.Invoke(viewId);
            }
        }

        static void RegisterLocalPlayer()
        {
            localPlayer = new CYGPlayer
            {
                PlayerId = seatSessionId,
                Nickname = string.IsNullOrEmpty(pendingNickname) ? DefaultNickname() : pendingNickname,
                IsLocal = true,
                IsMasterClient = true,
                ActorNumber = DeriveActorNumber(seatSessionId),
            };

            if (localPlayer.ActorNumber == 0)
            {
                localPlayer.ActorNumber = 1;
            }

            localPlayer.CustomProperties["room"] = RoomName;
            players.Clear();
            players[localPlayer.ActorNumber] = localPlayer;
        }

        static void TrackRosterEvent(string eventName, ArraySegment<byte> body)
        {
            if (string.IsNullOrEmpty(eventName) || localPlayer == null)
            {
                return;
            }

            try
            {
                if (eventName == EventPlayerJoin)
                {
                    reader.Attach(body.Array, body.Offset, body.Count);
                    int actor = (int)reader.ReadUInt32();
                    string nickname = reader.Remaining > 0 ? reader.ReadString() : string.Empty;
                    if (actor <= 0 || actor == localPlayer.ActorNumber)
                    {
                        return;
                    }

                    CYGPlayer remote = new CYGPlayer
                    {
                        ActorNumber = actor,
                        PlayerId = "actor-" + actor,
                        Nickname = nickname,
                    };

                    players[actor] = remote;
                    ReevaluateMasterClient();
                }
                else if (eventName == EventPlayerLeave)
                {
                    reader.Attach(body.Array, body.Offset, body.Count);
                    int actor = (int)reader.ReadUInt32();
                    if (players.Remove(actor) && actor != localPlayer.ActorNumber)
                    {
                        ReevaluateMasterClient();
                    }
                }
            }
            catch (Exception error)
            {
                Debug.LogWarning("[CYG] roster event \"" + eventName + "\" had an unexpected shape: " + error.Message);
            }
        }

        static void ReevaluateMasterClient()
        {
            int lowest = int.MaxValue;
            foreach (KeyValuePair<int, CYGPlayer> entry in players)
            {
                if (entry.Key < lowest)
                {
                    lowest = entry.Key;
                }
            }

            foreach (KeyValuePair<int, CYGPlayer> entry in players)
            {
                entry.Value.IsMasterClient = lowest != int.MaxValue && entry.Key == lowest;
            }
        }

        static bool TrySplitEventName(ArraySegment<byte> payload, out string eventName, out ArraySegment<byte> body)
        {
            eventName = string.Empty;
            body = payload;

            if (payload.Count < 1)
            {
                return false;
            }

            byte marker = payload.Array[payload.Offset];
            int nameLength;
            int headerSize;

            if (marker >= 0xA0 && marker <= 0xBF)
            {
                nameLength = marker & 0x1F;
                headerSize = 1;
            }
            else if (marker == 0xD9)
            {
                if (payload.Count < 2)
                {
                    return false;
                }

                nameLength = payload.Array[payload.Offset + 1];
                headerSize = 2;
            }
            else if (marker == 0xDA)
            {
                if (payload.Count < 3)
                {
                    return false;
                }

                nameLength = (payload.Array[payload.Offset + 1] << 8) | payload.Array[payload.Offset + 2];
                headerSize = 3;
            }
            else if (marker < 0x80)
            {
                // CYG's own writers prefix the name with an LEB128 length.
                reader.Attach(payload.Array, payload.Offset, payload.Count);
                try
                {
                    nameLength = (int)reader.ReadUInt32();
                }
                catch (Exception)
                {
                    return false;
                }

                headerSize = reader.BytesRead;
            }
            else
            {
                return false;
            }

            if (nameLength <= 0 || nameLength > MaxEventNameBytes || headerSize + nameLength > payload.Count)
            {
                return false;
            }

            // Binary room payloads share this channel, so a name must look like a name.
            for (int i = 0; i < nameLength; i++)
            {
                byte character = payload.Array[payload.Offset + headerSize + i];
                if (character < 0x20 || character > 0x7E)
                {
                    return false;
                }
            }

            eventName = Encoding.UTF8.GetString(payload.Array, payload.Offset + headerSize, nameLength);
            body = new ArraySegment<byte>(payload.Array, payload.Offset + headerSize + nameLength, payload.Count - headerSize - nameLength);
            return true;
        }

        static void WriteJoinHandshakeBody(CYGBitWriter target, string token)
        {
            string reconnectionToken = token ?? string.Empty;
            if (reconnectionToken.Length > byte.MaxValue)
            {
                reconnectionToken = reconnectionToken.Substring(0, byte.MaxValue);
                Debug.LogWarning("[CYG] reconnection token truncated to 255 characters for the join handshake.");
            }

            byte[] tokenBytes = Encoding.UTF8.GetBytes(reconnectionToken);
            target.WriteByte((byte)tokenBytes.Length);
            target.WriteRaw(new ArraySegment<byte>(tokenBytes, 0, tokenBytes.Length));
            target.WriteByte((byte)SerializerId.Length);
            for (int i = 0; i < SerializerId.Length; i++)
            {
                target.WriteByte((byte)SerializerId[i]);
            }

            target.WriteByte(0x00);
        }

        static string BuildMatchmakeBody(CYGServerSettings settings, string nickname)
        {
            StringBuilder builder = new StringBuilder(96);
            builder.Append('{');
            builder.Append("\"playerName\":\"").Append(EscapeJson(nickname)).Append('"');
            builder.Append(",\"maxPlayers\":").Append(settings.MaxPlayers.ToString(System.Globalization.CultureInfo.InvariantCulture));
            builder.Append(",\"protocolVersion\":").Append(((int)settings.ProtocolVersion).ToString(System.Globalization.CultureInfo.InvariantCulture));
            builder.Append('}');
            return builder.ToString();
        }

        static string EscapeJson(string value)
        {
            if (string.IsNullOrEmpty(value))
            {
                return string.Empty;
            }

            StringBuilder builder = new StringBuilder(value.Length + 8);
            for (int i = 0; i < value.Length; i++)
            {
                char c = value[i];
                switch (c)
                {
                    case '"': builder.Append("\\\""); break;
                    case '\\': builder.Append("\\\\"); break;
                    case '\n': builder.Append("\\n"); break;
                    case '\r': builder.Append("\\r"); break;
                    case '\t': builder.Append("\\t"); break;
                    default:
                        if (c < ' ')
                        {
                            builder.Append("\\u").Append(((int)c).ToString("x4"));
                        }
                        else
                        {
                            builder.Append(c);
                        }

                        break;
                }
            }

            return builder.ToString();
        }

        static string ReadJsonString(string json, string key)
        {
            string needle = "\"" + key + "\"";
            int at = json.IndexOf(needle, StringComparison.Ordinal);
            if (at < 0)
            {
                return string.Empty;
            }

            at = json.IndexOf(':', at + needle.Length);
            if (at < 0)
            {
                return string.Empty;
            }

            at++;
            while (at < json.Length && (json[at] == ' ' || json[at] == '\t' || json[at] == '\n' || json[at] == '\r'))
            {
                at++;
            }

            if (at >= json.Length || json[at] != '"')
            {
                return string.Empty;
            }

            at++;
            StringBuilder builder = new StringBuilder(32);
            bool escaped = false;
            while (at < json.Length)
            {
                char c = json[at++];
                if (escaped)
                {
                    builder.Append(c == 'n' ? '\n' : (c == 't' ? '\t' : (c == 'r' ? '\r' : c)));
                    escaped = false;
                    continue;
                }

                if (c == '\\')
                {
                    escaped = true;
                    continue;
                }

                if (c == '"')
                {
                    break;
                }

                builder.Append(c);
            }

            return builder.ToString();
        }

        static int ReadJsonInt(string json, string key)
        {
            string needle = "\"" + key + "\"";
            int at = json.IndexOf(needle, StringComparison.Ordinal);
            if (at < 0)
            {
                return 0;
            }

            at = json.IndexOf(':', at + needle.Length);
            if (at < 0)
            {
                return 0;
            }

            at++;
            while (at < json.Length && (json[at] == ' ' || json[at] == '\t'))
            {
                at++;
            }

            int sign = 1;
            if (at < json.Length && json[at] == '-')
            {
                sign = -1;
                at++;
            }

            int value = 0;
            bool any = false;
            while (at < json.Length && json[at] >= '0' && json[at] <= '9')
            {
                value = value * 10 + (json[at] - '0');
                at++;
                any = true;
            }

            return any ? value * sign : 0;
        }

        static void WriteUInt32BigEndian(byte[] target, int at, uint value)
        {
            target[at] = (byte)(value >> 24);
            target[at + 1] = (byte)(value >> 16);
            target[at + 2] = (byte)(value >> 8);
            target[at + 3] = (byte)value;
        }

        static int DeriveActorNumber(string sessionId)
        {
            if (string.IsNullOrEmpty(sessionId))
            {
                return 1;
            }

            // If sessionId starts with "p" followed by a number and "_", parse the server-assigned player id directly
            if (sessionId.StartsWith("p") && sessionId.IndexOf('_') > 1)
            {
                int underscore = sessionId.IndexOf('_');
                int parsedId;
                if (int.TryParse(sessionId.Substring(1, underscore - 1), out parsedId) && parsedId > 0)
                {
                    return parsedId;
                }
            }

            // The room server assigns short ids inside the guest module and never
            // publishes them schema-free, so the client derives a stable local actor.
            uint hash = 2166136261;
            for (int i = 0; i < sessionId.Length; i++)
            {
                hash ^= sessionId[i];
                hash *= 16777619;
            }

            return (int)(hash & 0x3FFFu) + 1;
        }

        static string ReadRoomFromSocketUrl(string websocketUrl)
        {
            // The room server keys the session on the last path segment of the upgrade url.
            string path = websocketUrl;
            int query = path.IndexOf('?');
            if (query >= 0)
            {
                path = path.Substring(0, query);
            }

            int slash = path.LastIndexOf('/');
            return slash >= 0 && slash + 1 < path.Length ? path.Substring(slash + 1) : string.Empty;
        }

        static string DefaultNickname()
        {
            try
            {
                string candidate = Environment.UserName;
                return string.IsNullOrEmpty(candidate) ? "Player" : candidate;
            }
            catch (Exception)
            {
                // WebGL has no user account to read.
                return "Player";
            }
        }

        static void AbandonSeatRequest()
        {
            UnityWebRequest request = seatRequest;
            seatRequest = null;
            if (request == null)
            {
                return;
            }

            try
            {
                request.Abort();
                request.Dispose();
            }
            catch (Exception error)
            {
                Debug.LogWarning("[CYG] pending matchmake request could not be abandoned: " + error.Message);
            }
        }

        static void ClearRoomState()
        {
            joinHandshakeComplete = false;
            localPlayer = null;
            players.Clear();
            CYGViewTable.Clear();
            viewIndex = 0;
            Latency = 0f;
            lastPingAt = 0;
        }

        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.SubsystemRegistration)]
        static void ResetDomain()
        {
            Disconnect();
            if (transport != null)
            {
                transport.Close();
                transport = null;
            }
            ClearRoomState();
            AbandonSeatRequest();
            lastPingAt = 0;
            lastPingSentAt = 0;
            viewIndex = 0;
            joinHandshakeComplete = false;
            localPlayer = null;
            players.Clear();
            seatRequest = null;
            seatSessionId = null;
            seatRoomId = null;
            seatProcessId = null;
            seatToken = null;
            pendingNickname = null;
            pendingRoomName = null;

            OnConnectedEvent = null;
            OnDisconnectedEvent = null;
            OnErrorEvent = null;
            OnRoomStateReceived = null;
            OnRoomEvent = null;
            OnViewDelta = null;
            OnRpcReceived = null;
            OnObjectSpawned = null;
            OnObjectDespawned = null;
            OnRoomDataBytes = null;
            OnRewindHitReceived = null;
            OnConnectedToServer = null;
            OnJoinedRoom = null;
        }
    }
}
