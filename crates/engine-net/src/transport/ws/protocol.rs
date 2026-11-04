
use crate::framing::OpCode;

pub(super) const PROTO_JOIN_ROOM: u8 = 10;
pub(super) const PROTO_ERROR: u8 = 11;
pub(super) const PROTO_LEAVE_ROOM: u8 = 12;
pub(super) const PROTO_ROOM_DATA: u8 = 13;
pub(super) const PROTO_ROOM_STATE: u8 = 14;
pub(super) const PROTO_ROOM_STATE_PATCH: u8 = 15;
pub(super) const PROTO_ROOM_DATA_BYTES: u8 = 17;
pub(super) const PROTO_PING: u8 = 18;
pub(super) const PROTO_ROOM_REQUEST: u8 = 21;
pub(super) const PROTO_ROOM_RESPONSE: u8 = 22;

pub(super) const PROTOCOL_CODE_MASK: u8 = 0x1F;

pub mod close_code {
    pub const CONSENTED: u16 = 4000;
    pub const SERVER_SHUTDOWN: u16 = 4001;
    pub const WITH_ERROR: u16 = 4002;
    pub const FAILED_TO_RECONNECT: u16 = 4003;
    pub const MAY_TRY_RECONNECT: u16 = 4010;
    /// Send queue overflowed repeatedly — the peer stopped draining.
    pub const SLOW_CONSUMER: u16 = 4004;
}

pub mod response_status {
    pub const OK: u8 = 0;
    pub const REJECTED: u8 = 1;
    pub const ERROR: u8 = 2;
}

pub const MATCHMAKE_METHODS: [&str; 5] =
    ["joinOrCreate", "create", "join", "joinById", "reconnect"];

pub(super) const SERIALIZER_ID: &[u8] = b"schema";

pub(super) fn wire_opcode_for(opcode: OpCode) -> u8 {
    match opcode {
        OpCode::StateDelta => PROTO_ROOM_STATE_PATCH,
        OpCode::StateSnapshot => PROTO_ROOM_STATE,
        OpCode::RoomDataBytes => PROTO_ROOM_DATA_BYTES,
        OpCode::RoomResponse => PROTO_ROOM_RESPONSE,
        OpCode::Error => PROTO_ERROR,
        OpCode::Ping => PROTO_PING,
        _ => PROTO_ROOM_DATA,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_codes_match_the_sdk_contract() {
        assert_eq!(PROTO_JOIN_ROOM, 10);
        assert_eq!(PROTO_ERROR, 11);
        assert_eq!(PROTO_LEAVE_ROOM, 12);
        assert_eq!(PROTO_ROOM_DATA, 13);
        assert_eq!(PROTO_ROOM_STATE, 14);
        assert_eq!(PROTO_ROOM_STATE_PATCH, 15);
        assert_eq!(PROTO_ROOM_DATA_BYTES, 17);
        assert_eq!(PROTO_PING, 18);
        assert_eq!(PROTO_ROOM_REQUEST, 21);
        assert_eq!(PROTO_ROOM_RESPONSE, 22);
        assert_eq!(PROTOCOL_CODE_MASK, 0x1F);
    }

    #[test]
    fn internal_tags_map_onto_wire_codes() {
        assert_eq!(
            wire_opcode_for(OpCode::RoomDataBytes),
            PROTO_ROOM_DATA_BYTES
        );
        assert_eq!(wire_opcode_for(OpCode::StateDelta), PROTO_ROOM_STATE_PATCH);
        assert_eq!(wire_opcode_for(OpCode::StateSnapshot), PROTO_ROOM_STATE);
        assert_eq!(wire_opcode_for(OpCode::Ping), PROTO_PING);
        assert_eq!(wire_opcode_for(OpCode::Error), PROTO_ERROR);
    }
}
