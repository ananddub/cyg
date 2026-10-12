using System;
using System.IO;
using UnityEngine;

namespace OpenOxide
{
    /// <summary>
    /// Main entry point for connecting to OpenOxide Gaming Servers from Unity.
    /// Supports low-latency UDP datagram transport and WebSocket failover.
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

        /// <summary>
        /// Join or create a match room as an active combatant.
        /// </summary>
        public OpenOxideRoom JoinOrCreate(string roomName)
        {
            OpenOxideRoom room = new OpenOxideRoom(roomName, isSpectator: false);
            OpenOxideUdpSocket socket = new OpenOxideUdpSocket();
            socket.Connect(Host, Port);
            room.AttachSocket(socket);
            return room;
        }

        /// <summary>
        /// Join a match room in read-only Spectator / Observer mode.
        /// </summary>
        public OpenOxideRoom Spectate(string roomName)
        {
            OpenOxideRoom room = new OpenOxideRoom(roomName, isSpectator: true);
            OpenOxideUdpSocket socket = new OpenOxideUdpSocket();
            socket.Connect(Host, Port);
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
