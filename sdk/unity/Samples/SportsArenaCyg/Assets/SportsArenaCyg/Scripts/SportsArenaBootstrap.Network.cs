using System;
using System.Text;
using CookYourGame;
using CookYourGame.Sim;
using UnityEngine;

namespace CookYourGame.SportsArena
{
    /// <summary>
    /// Optional two-client mode. Each instance still runs the authoritative
    /// deterministic simulation locally, but the local keyboard input is
    /// published through the OpenOxide input hub and the other instance applies
    /// it to its remote fighter. The backend therefore relays real key input
    /// between the two running players instead of using a local bot for P2.
    /// </summary>
    public sealed partial class SportsArenaBootstrap
    {
        [Header("Network (OpenOxide backend)")]
        [Tooltip("Start with -network (or CYG_NETWORK=1) to join a two-client input relay room.")]
        public bool networkMode;

        [Tooltip("OpenOxide WebSocket host. Override with CYG_SERVER_HOST.")]
        public string networkServer = "127.0.0.1";

        [Tooltip("OpenOxide game port (the HTTP/WS transport port). Override with CYG_SERVER_PORT.")]
        public int networkPort = 7777;

        [Tooltip("Input-hub room name. Override with CYG_ROOM.")]
        public string networkRoom = "cyg_sports";

        public bool NetworkMode => networkMode;

        /// <summary>Fighter slot driven by this window's keyboard (0 = P1, 1 = P2).</summary>
        public int LocalPlayerIndex { get; private set; }

        /// <summary>Fighter slot driven by the peer's relayed keyboard input.</summary>
        public int RemotePlayerIndex { get; private set; } = 1;

        public bool NetworkConnected => networkMode && CYGNetwork.IsConnected;
        public bool HasRemotePlayer { get; private set; }
        public bool HasRemoteInput => _hasRemoteInput;

        private CYGNetworkManager _networkManager;
        private readonly byte[] _inputPacket = new byte[16 + CYGPlayerInput.SizeInBytes * 3];
        private readonly byte[] _inputRoomId = new byte[8];
        private string _inputRoomName;
        private CYGPlayerInput _previousInput1;
        private CYGPlayerInput _previousInput2;
        private bool _hasPreviousInput1;
        private bool _hasPreviousInput2;
        private int _remoteActor;
        private CYGPlayerInput _remoteInput;
        private bool _hasRemoteInput;
        private bool _sawRemoteInputActivity;
        private float _lastRemoteInputAt;
        private bool _networkStarted;

        private void InitializeNetworkMode()
        {
            string[] args = Environment.GetCommandLineArgs();
            for (int i = 0; i < args.Length; i++)
            {
                string arg = args[i];
                if (arg == "-network" || arg == "-cygNetwork"
                    || string.Equals(arg, "CYG_NETWORK=1", StringComparison.OrdinalIgnoreCase))
                {
                    networkMode = true;
                }
            }

            if (Environment.GetEnvironmentVariable("CYG_NETWORK") == "1")
            {
                networkMode = true;
            }

            string host = Environment.GetEnvironmentVariable("CYG_SERVER_HOST");
            if (!string.IsNullOrEmpty(host))
            {
                networkServer = host;
            }

            string room = Environment.GetEnvironmentVariable("CYG_ROOM");
            if (!string.IsNullOrEmpty(room))
            {
                networkRoom = room;
            }

            string port = Environment.GetEnvironmentVariable("CYG_SERVER_PORT");
            int parsedPort;
            if (!string.IsNullOrEmpty(port) && int.TryParse(port, out parsedPort) && parsedPort > 0)
            {
                networkPort = parsedPort;
            }

            if (networkMode)
            {
                playerTwoIsBot = false;
            }
        }

        private void StartNetworkMode()
        {
            if (!networkMode || _networkStarted)
            {
                return;
            }

            _networkStarted = true;
            playerTwoIsBot = false;
            if (Simulation != null)
            {
                Simulation.Players[0].IsBot = false;
                Simulation.Players[1].IsBot = false;
            }

            // Keep the network driver on its own persistent object so the
            // arena root can be destroyed normally without killing the socket.
            GameObject networkObject = new GameObject("CYG Sports Arena Network");
            UnityEngine.Object.DontDestroyOnLoad(networkObject);

            _networkManager = networkObject.AddComponent<CYGNetworkManager>();
            _networkManager.ServerUrl = "http://" + networkServer + ":" + networkPort;
            _networkManager.RoomName = networkRoom;
            _networkManager.AutoSpawnPlayer = false;

            CYGNetwork.OnInputTickReceived += HandleNetworkInputTick;
            CYGNetwork.OnJoinedRoom += HandleNetworkJoined;
            CYGNetwork.OnDisconnectedEvent += HandleNetworkDisconnected;

            // Force the WebSocket transport. The raw input hub is available on
            // the same port and the server accepts a direct CYG room upgrade,
            // so both windows can use the exact same session/actor derivation.
            CYGNetwork.TransportFactory = () => new CYGWebSocketTransport();
            CYGNetwork.ConnectToMatch(networkRoom, string.Empty, networkServer, networkPort);

            Debug.Log("[SportsArena] Connecting to OpenOxide input hub at ws://"
                + networkServer + ":" + networkPort + "/" + networkRoom);
        }

        private void ShutdownNetworkMode()
        {
            if (!_networkStarted)
            {
                return;
            }

            CYGNetwork.OnInputTickReceived -= HandleNetworkInputTick;
            CYGNetwork.OnJoinedRoom -= HandleNetworkJoined;
            CYGNetwork.OnDisconnectedEvent -= HandleNetworkDisconnected;
            if (_networkManager != null)
            {
                _networkManager.Shutdown();
            }
            CYGNetwork.TransportFactory = null;
            _networkStarted = false;
        }

        private void StepNetworkInputs()
        {
            // Both windows use the same WASD/J/K/E/L/Space layout; the slot
            // (P1 or P2) is assigned from the peer's actor number.
            CYGPlayerInput local = ReadInput(0, Tick).Value;

            for (int i = 0; i < _inputs.Length; i++)
            {
                if (i == LocalPlayerIndex)
                {
                    _inputs[i].Value = local;
                }
                else if (i == RemotePlayerIndex)
                {
                    _inputs[i].Value = ReadRemoteInput();
                }
                else
                {
                    // P3-P6 stay local bots. They must not pick up the
                    // keyboard/arrow input of the two human slots.
                    _inputs[i] = default;
                }
            }

            SendLocalInput(local);
        }

        /// <summary>
        /// Builds the same 16-byte input-hub header as CYGInputCollector and
        /// sends the current input plus two redundant predecessors.
        /// </summary>
        private void SendLocalInput(in CYGPlayerInput current)
        {
            if (!CYGNetwork.IsConnected || CYGNetwork.LocalActorNumber <= 0)
            {
                return;
            }

            string room = CYGNetwork.RoomName;
            if (string.IsNullOrEmpty(room))
            {
                return;
            }

            if (!string.Equals(room, _inputRoomName, StringComparison.Ordinal))
            {
                _inputRoomName = room;
                Array.Clear(_inputRoomId, 0, _inputRoomId.Length);
                byte[] roomBytes = Encoding.UTF8.GetBytes(room);
                Buffer.BlockCopy(roomBytes, 0, _inputRoomId, 0, Math.Min(roomBytes.Length, _inputRoomId.Length));
            }

            _inputPacket[0] = CYGNetwork.CYG_WIRE_INPUT_TICK;
            Buffer.BlockCopy(_inputRoomId, 0, _inputPacket, 1, _inputRoomId.Length);
            _inputPacket[9] = (byte)((Tick >> 24) & 0xFF);
            _inputPacket[10] = (byte)((Tick >> 16) & 0xFF);
            _inputPacket[11] = (byte)((Tick >> 8) & 0xFF);
            _inputPacket[12] = (byte)(Tick & 0xFF);
            int actor = CYGNetwork.LocalActorNumber;
            _inputPacket[13] = (byte)((actor >> 8) & 0xFF);
            _inputPacket[14] = (byte)(actor & 0xFF);

            int offset = 16;
            current.WriteTo(_inputPacket, offset);
            offset += CYGPlayerInput.SizeInBytes;
            if (_hasPreviousInput1)
            {
                _previousInput1.WriteTo(_inputPacket, offset);
                offset += CYGPlayerInput.SizeInBytes;
                if (_hasPreviousInput2)
                {
                    _previousInput2.WriteTo(_inputPacket, offset);
                    offset += CYGPlayerInput.SizeInBytes;
                }
            }

            _inputPacket[15] = (byte)(offset - 16);
            CYGNetwork.SendRaw(CYGNetwork.CYG_WIRE_INPUT_TICK,
                new ArraySegment<byte>(_inputPacket, 0, offset));

            _previousInput2 = _previousInput1;
            _hasPreviousInput2 = _hasPreviousInput1;
            _previousInput1 = current;
            _hasPreviousInput1 = true;
        }

        private CYGPlayerInput ReadRemoteInput()
        {
            if (!_hasRemoteInput
                || Time.realtimeSinceStartup - _lastRemoteInputAt > 0.5f)
            {
                return new CYGPlayerInput(Tick, 0, 0, 0);
            }

            return new CYGPlayerInput(Tick, _remoteInput.Buttons, _remoteInput.MoveX, _remoteInput.MoveY);
        }

        private void HandleNetworkJoined(string room)
        {
            int actor = CYGNetwork.LocalActorNumber;
            Debug.Log("[SportsArena] Joined OpenOxide input-hub room '" + networkRoom + "' as actor " + actor);
        }

        private void HandleNetworkDisconnected(string reason)
        {
            _hasRemoteInput = false;
            HasRemotePlayer = false;
            _remoteActor = 0;
            Debug.LogWarning("[SportsArena] OpenOxide disconnected: " + reason);
        }

        private void HandleNetworkInputTick(ArraySegment<byte> payload)
        {
            byte[] data = payload.Array;
            if (data == null || payload.Count < 16 + CYGPlayerInput.SizeInBytes)
            {
                return;
            }

            int offset = payload.Offset;
            if (data[offset] != CYGNetwork.CYG_WIRE_INPUT_TICK)
            {
                return;
            }

            int actor = (data[offset + 13] << 8) | data[offset + 14];
            int inputLength = data[offset + 15];
            int localActor = CYGNetwork.LocalActorNumber;
            if (actor <= 0 || actor == localActor || inputLength < CYGPlayerInput.SizeInBytes)
            {
                return;
            }

            CYGPlayerInput remote = CYGPlayerInput.ReadFrom(
                data,
                offset + 16).Clamped();

            if (actor != _remoteActor)
            {
                _remoteActor = actor;
                HasRemotePlayer = true;
                Debug.Log("[SportsArena] first remote input from actor " + actor);
                ResolveNetworkRoles(actor);
            }

            _remoteInput = remote;
            _hasRemoteInput = true;
            _lastRemoteInputAt = Time.realtimeSinceStartup;

            if (!_sawRemoteInputActivity
                && (remote.Buttons != 0 || remote.MoveX != 0 || remote.MoveY != 0))
            {
                _sawRemoteInputActivity = true;
                Debug.Log("[SportsArena] Remote key input received for P"
                    + (RemotePlayerIndex + 1)
                    + " (move=" + remote.MoveX + "," + remote.MoveY
                    + " buttons=" + remote.Buttons + ")");
            }
        }

        private void ResolveNetworkRoles(int remoteActor)
        {
            int localActor = CYGNetwork.LocalActorNumber;
            if (localActor <= 0 || remoteActor <= 0 || localActor == remoteActor)
            {
                return;
            }

            // Both windows independently compare the two derived actor ids, so
            // they always agree on who controls P1 and who controls P2.
            LocalPlayerIndex = localActor < remoteActor ? 0 : 1;
            RemotePlayerIndex = 1 - LocalPlayerIndex;
            if (Simulation != null)
            {
                Simulation.Players[0].IsBot = false;
                Simulation.Players[1].IsBot = false;
            }

            Debug.Log("[SportsArena] Network roles: local actor " + localActor
                + " -> P" + (LocalPlayerIndex + 1)
                + ", remote actor " + remoteActor
                + " -> P" + (RemotePlayerIndex + 1));
        }
    }
}
