using System;
using System.IO;
using UnityEngine;

namespace OpenOxide
{
    public enum TransportType
    {
        Auto,       // UDP on Native/Editor, WebSocket on WebGL
        Udp,
        WebSocket
    }

    /// <summary>
    /// Main entry point for connecting to OpenOxide Gaming Servers from Unity.
    /// Supports low-latency UDP datagram transport and WebSocket failover/WebGL.
    /// </summary>
    public class OpenOxideClient
    {
        public string Host { get; private set; }
        public int Port { get; private set; }

        public OpenOxideClient(string host = "127.0.0.1", int port = 7778)
        {
            Host = host;
            Port = port;
        }

        public IOpenOxideSocket CreateSocket(TransportType transport, string roomName, bool isSpectator)
        {
            bool useWs = false;
            if (transport == TransportType.WebSocket)
            {
                useWs = true;
            }
            else if (transport == TransportType.Auto)
            {
#if UNITY_WEBGL && !UNITY_EDITOR
                useWs = true;
#else
                useWs = (Port == 8090 || Port == 7777);
#endif
            }

            if (useWs)
            {
                var ws = new OpenOxideWebSocket();
                string protocol = "ws";
                int wsPort = (Port == 7778) ? 8090 : Port;
                string url = $"{protocol}://{Host}:{wsPort}/{roomName}";
                if (isSpectator) url += "?spectator=1";
                ws.Connect(url);
                return ws;
            }
            else
            {
                var udp = new OpenOxideUdpSocket();
                udp.Connect(Host, Port);
                return udp;
            }
        }

        /// <summary>
        /// Join or create a match room as an active combatant.
        /// </summary>
        public OpenOxideRoom JoinOrCreate(string roomName, TransportType transport = TransportType.Auto)
        {
            OpenOxideRoom room = new OpenOxideRoom(roomName, isSpectator: false);
            var socket = CreateSocket(transport, roomName, isSpectator: false);
            room.AttachSocket(socket);
            return room;
        }

        /// <summary>
        /// Join a match room in read-only Spectator / Observer mode.
        /// </summary>
        public OpenOxideRoom Spectate(string roomName, TransportType transport = TransportType.Auto)
        {
            OpenOxideRoom room = new OpenOxideRoom(roomName, isSpectator: true);
            var socket = CreateSocket(transport, roomName, isSpectator: true);
            room.AttachSocket(socket);
            return room;
        }

        /// <summary>
        /// Load a .oxr match replay file for instant kill-cams and match playback.
        /// </summary>
        public static OpenOxideReplayPlayer LoadReplay(string jsonOrFilePath)
        {
            string json = jsonOrFilePath;
            if (File.Exists(jsonOrFilePath))
            {
                json = File.ReadAllText(jsonOrFilePath);
            }

            MatchReplayData data = JsonUtility.FromJson<MatchReplayData>(json);
            return new OpenOxideReplayPlayer(data);
        }
    }
}
