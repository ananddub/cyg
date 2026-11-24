// CYG Unity SDK — relay-only voice handle.
//
// The native-UDP lane carries pre-encoded Opus frames; the host owns peer
// membership, spatial/team routing and mute/deafen verdicts. Mic capture,
// Opus encoding/decoding and playback stay with the game (Unity's
// `Microphone` class + an Opus codec), mirroring the Godot relay-only port.
//
// Wire:
//   team    [0x56]['T'][hash u32 BE]        (0 = leave team)
//   mute    [0x56]['M'][1/0]
//   deafen  [0x56]['D'][1/0]
//   voice   [0x56][sender u32 BE][seq u16 BE][timestamp u32 BE][mode][flags][len u16 BE][opus…]

using System;

namespace CYG
{
    /// <summary>Voice channel byte, matching the host's <c>VoiceChannelMode</c>.</summary>
    public enum VoiceChannel : byte
    {
        Proximity = 0,
        Global = 1,
        TeamOnly = 2,
        Whisper = 3,
    }

    /// <summary>
    /// Relay-only voice handle for Unity. Team labels may be strings
    /// (<c>"red"</c>, <c>"squad-a"</c>) or numbers; the SDK hashes them to a
    /// u32 so routing is a single integer compare per packet.
    /// </summary>
    public sealed class CygVoice : IDisposable
    {
        private readonly CygClient _client;

        internal CygVoice(CygClient client)
        {
            _client = client;
            _client.OnVoice += HandlePacket;
        }

        /// <summary>Starts muted until the game enables the relay path.</summary>
        public bool Enabled { get; private set; }

        /// <summary>The raw-UDP lane is relay-only, so no peer connection is needed.</summary>
        public static bool IsSupported => true;

        /// <summary>No WebRTC mesh here — every speaker rides the server relay.</summary>
        public bool IsP2P(long peerId) => false;

        /// <summary>
        /// A routed voice frame from another player. The packet is the full
        /// 15-byte header + Opus payload; the speaker id is the first argument.
        /// </summary>
        public event Action<long, byte[]> OnPacket;

        public void Enable() => Enabled = true;
        public void Disable() => Enabled = false;

        /// <summary>Set the squad/team used by <see cref="VoiceChannel.TeamOnly"/>.</summary>
        public void SetTeam(string teamId) => _client.SetVoiceTeam(teamId);
        /// <summary>Number overload — stringified, then hashed like the web SDK.</summary>
        public void SetTeam(int teamId) => _client.SetVoiceTeam(teamId);
        /// <summary>Leave the current squad (routes as "no team").</summary>
        public void LeaveTeam() => _client.SetVoiceTeam((string)null);

        public void SetMuted(bool muted) => _client.SetVoiceMuted(muted);
        public void SetDeafened(bool deafened) => _client.SetVoiceDeafened(deafened);

        /// <summary>Stream one Opus frame on the given channel.</summary>
        public void Send(VoiceChannel mode, ushort seq, uint timestamp, byte flags, byte[] opusPayload)
        {
            if (!Enabled) return;
            _client.SendVoice(mode, seq, timestamp, flags, opusPayload);
        }

        private void HandlePacket(long speakerId, byte[] packet)
        {
            if (Enabled) OnPacket?.Invoke(speakerId, packet);
        }

        public void Dispose()
        {
            Enabled = false;
            _client.OnVoice -= HandlePacket;
        }
    }
}
