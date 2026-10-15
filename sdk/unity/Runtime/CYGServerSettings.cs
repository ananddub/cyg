using System;
using System.Globalization;
using UnityEngine;

namespace CookYourGame
{
    /// <summary>Server endpoints and room defaults for a CookYourGame client build.</summary>
    [CreateAssetMenu(fileName = "CYGServerSettings", menuName = "CookYourGame/Server Settings", order = 0)]
    public class CYGServerSettings : ScriptableObject
    {
        /// <summary>Room name used when the game joins without naming one.</summary>
        public const string DefaultRoomName = "cyg_room";

        /// <summary>Matchmake method that attaches to a room or starts it.</summary>
        public const string DefaultMatchmakeMethod = "joinOrCreate";

        [Tooltip("Base HTTP endpoint of the room server, e.g. http://localhost:4000")]
        [SerializeField] string serverHttpUrl = "http://localhost:4000";

        [Tooltip("Path prefix the game socket is upgraded on, e.g. /ws")]
        [SerializeField] string serverWsPath = "/ws";

        [Tooltip("Room joined when the game does not name one")]
        [SerializeField] string roomName = DefaultRoomName;

        [Tooltip("Room capacity advertised to the matchmaker")]
        [SerializeField] int maxPlayers = 8;

        [Tooltip("Wire protocol revision the client speaks")]
        [SerializeField] ushort protocolVersion = 1;

        [Tooltip("Matchmake method: joinOrCreate, join, create, joinById, rejoin or reconnect")]
        [SerializeField] string matchmakeMethod = DefaultMatchmakeMethod;

        /// <summary>Base HTTP endpoint of the control plane.</summary>
        public string ServerHttpUrl
        {
            get { return serverHttpUrl; }
            set { serverHttpUrl = value; }
        }

        /// <summary>Path prefix used for the game socket upgrade.</summary>
        public string ServerWsPath
        {
            get { return serverWsPath; }
            set { serverWsPath = value; }
        }

        /// <summary>Default room name.</summary>
        public string RoomName
        {
            get { return roomName; }
            set { roomName = value; }
        }

        /// <summary>Advertised room capacity.</summary>
        public int MaxPlayers
        {
            get { return maxPlayers; }
            set { maxPlayers = value; }
        }

        /// <summary>Wire protocol revision the client speaks.</summary>
        public ushort ProtocolVersion
        {
            get { return protocolVersion; }
            set { protocolVersion = value; }
        }

        /// <summary>Matchmake method used by ConnectToRoom.</summary>
        public string MatchmakeMethod
        {
            get { return string.IsNullOrEmpty(matchmakeMethod) ? DefaultMatchmakeMethod : matchmakeMethod; }
            set { matchmakeMethod = value; }
        }

        /// <summary>The socket base url mirrored from the http endpoint (http to ws, https to wss).</summary>
        public string WsUrl
        {
            get
            {
                Uri uri;
                if (!TryHttpUri(serverHttpUrl, out uri))
                {
                    return string.Empty;
                }

                string scheme = uri.Scheme == Uri.UriSchemeHttps ? "wss" : "ws";
                string baseurl = scheme + "://" + uri.Authority;
                string path = TrimSlashes(serverWsPath);
                return path.Length == 0 ? baseurl : baseurl + "/" + path;
            }
        }

        /// <summary>Builds the seat-reservation url for a method and room.</summary>
        public string MatchmakeUrl(string method, string room)
        {
            string baseurl = TrimSlashes(serverHttpUrl);
            if (string.IsNullOrEmpty(baseurl))
            {
                Debug.LogError("[CYG] serverHttpUrl is empty; cannot build a matchmake url.");
                return string.Empty;
            }

            return baseurl + "/api/matchmake/" + EscapeSegment(ResolveMethod(method)) + "/" + EscapeSegment(NormalizeRoomName(room));
        }

        /// <summary>Builds the game socket url for a reserved seat.</summary>
        public string GameSocketUrl(string processId, string roomId, string sessionId, string seatToken, int gamePort)
        {
            Uri source;
            if (!TryHttpUri(serverHttpUrl, out source))
            {
                Debug.LogError("[CYG] serverHttpUrl is not a valid http uri: " + serverHttpUrl);
                return string.Empty;
            }

            string scheme = source.Scheme == Uri.UriSchemeHttps ? "wss" : "ws";
            int port = gamePort > 0 ? gamePort : source.Port;
            string host = source.Host;
            string path = TrimSlashes(serverWsPath);
            string room = EscapeSegment(string.IsNullOrEmpty(roomId) ? NormalizeRoomName(roomName) : roomId);
            string process = string.IsNullOrEmpty(processId) ? "cyg" : EscapeSegment(processId);

            string url = scheme + "://" + host;
            if (port > 0 && port != (source.Scheme == Uri.UriSchemeHttps ? 443 : 80))
            {
                url += ":" + port.ToString(CultureInfo.InvariantCulture);
            }

            if (path.Length > 0)
            {
                url += "/" + path;
            }

            url += "/" + process + "/" + room;

            string query = string.Empty;
            if (!string.IsNullOrEmpty(sessionId))
            {
                query += "sessionId=" + EscapeSegment(sessionId);
            }

            if (!string.IsNullOrEmpty(seatToken))
            {
                query += query.Length == 0 ? "?" : "&";
                query += "token=" + EscapeSegment(seatToken);
            }

            return url + query;
        }

        /// <summary>Clamps a room name to the server rule: 1-64 letters, digits, '-' or '_'.</summary>
        public static string NormalizeRoomName(string room)
        {
            if (string.IsNullOrEmpty(room))
            {
                return DefaultRoomName;
            }

            char[] chars = new char[room.Length];
            int count = 0;
            for (int i = 0; i < room.Length && count < 64; i++)
            {
                char c = room[i];
                bool allowed = (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '-' || c == '_';
                if (allowed)
                {
                    chars[count++] = c;
                }
            }

            if (count == 0)
            {
                Debug.LogWarning("[CYG] room name \"" + room + "\" has no legal characters; using \"" + DefaultRoomName + "\".");
                return DefaultRoomName;
            }

            return new string(chars, 0, count);
        }

        string ResolveMethod(string method)
        {
            return string.IsNullOrEmpty(method) ? MatchmakeMethod : method;
        }

        static bool TryHttpUri(string url, out Uri uri)
        {
            uri = null;
            if (string.IsNullOrEmpty(url))
            {
                return false;
            }

            return Uri.TryCreate(url.TrimEnd('/'), UriKind.Absolute, out uri)
                && (uri.Scheme == Uri.UriSchemeHttp || uri.Scheme == Uri.UriSchemeHttps);
        }

        static string TrimSlashes(string value)
        {
            if (string.IsNullOrEmpty(value))
            {
                return string.Empty;
            }

            int start = 0;
            int end = value.Length;
            while (start < end && value[start] == '/')
            {
                start++;
            }

            while (end > start && value[end - 1] == '/')
            {
                end--;
            }

            return value.Substring(start, end - start);
        }

        static string EscapeSegment(string value)
        {
            return string.IsNullOrEmpty(value) ? string.Empty : Uri.EscapeDataString(value);
        }

        void OnValidate()
        {
            if (maxPlayers < 1)
            {
                maxPlayers = 1;
            }

            if (maxPlayers > 255)
            {
                maxPlayers = 255;
            }

            if (string.IsNullOrEmpty(roomName))
            {
                roomName = DefaultRoomName;
            }
        }
    }
}
