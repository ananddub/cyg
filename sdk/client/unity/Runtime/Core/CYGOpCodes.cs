namespace CookYourGame
{
    /// <summary>Frame opcodes carried in the 3-byte CYG packet header.</summary>
    public enum CYGOpCode : byte
    {
        /// <summary>Client heartbeat request.</summary>
        Ping = 0,

        /// <summary>Heartbeat acknowledgement (also the echoed ping).</summary>
        Pong = 1,

        /// <summary>Logical join request for a room.</summary>
        JoinRoom = 2,

        /// <summary>Logical leave request for a room.</summary>
        LeaveRoom = 3,

        /// <summary>Per-tick player input payload.</summary>
        PlayerInput = 4,

        /// <summary>Synced view state delta.</summary>
        StateDelta = 5,

        /// <summary>Remote procedure call on a view.</summary>
        CustomRpc = 6,

        /// <summary>Lag-compensated rewind hit registration.</summary>
        RewindHit = 7,

        /// <summary>Full state snapshot for (re)joins.</summary>
        StateSnapshot = 8,

        /// <summary>Room join handshake on the wire protocol.</summary>
        ProtoJoinRoom = 10,

        /// <summary>Room leave on the wire protocol.</summary>
        ProtoLeaveRoom = 12,

        /// <summary>Named room event on the wire protocol.</summary>
        ProtoRoomData = 13,

        /// <summary>Authoritative room state on the wire protocol.</summary>
        ProtoRoomState = 14,

        /// <summary>Incremental room state patch on the wire protocol.</summary>
        ProtoStatePatch = 15,

        /// <summary>Opaque binary room payload on the wire protocol.</summary>
        ProtoDataBytes = 17,

        /// <summary>
        /// Per-tick deterministic input-hub payload. Must stay distinct from
        /// the transport's PING/PONG wire code (18).
        /// </summary>
        InputTick = 19,

        /// <summary>Instantiates a registered prefab for every peer.</summary>
        SpawnObject = 20,

        /// <summary>Destroys a previously spawned object for every peer.</summary>
        DespawnObject = 21,
    }
}
