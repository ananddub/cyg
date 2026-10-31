using UnityEngine;

namespace CookYourGame
{
    /// <summary>
    /// Where a built client connects, imported from a game's console as a
    /// <c>.asset</c>. The field names are the serialized keys of that asset: Unity
    /// drops a key the class does not declare and leaves a declared field at its
    /// default when the asset omits it, so both lists must match exactly.
    /// </summary>
    [CreateAssetMenu(fileName = "CYGServerSettings", menuName = "CookYourGame/Server Settings")]
    public sealed class CYGServerSettings : ScriptableObject
    {
        [SerializeField] string serverHttpUrl = "http://127.0.0.1:7777";
        [SerializeField] string serverWsPath = "/ws";
        [SerializeField] string roomName = "";
        [SerializeField] int maxPlayers = 16;
        [SerializeField] ushort protocolVersion = 1;
        [SerializeField] string matchmakeMethod = "matchmake";

        public string ServerHttpUrl => serverHttpUrl;
        public string ServerWsPath => serverWsPath;
        public string RoomName => roomName;
        public int MaxPlayers => maxPlayers;
        public ushort ProtocolVersion => protocolVersion;
        public string MatchmakeMethod => matchmakeMethod;
    }
}
