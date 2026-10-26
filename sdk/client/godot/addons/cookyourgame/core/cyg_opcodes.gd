class_name CYGOpcodes
## Wire and canonical opcode constants for the CookYourGame protocol.
##
## Two opcode spaces exist. The *wire* code is the leading byte of each
## WebSocket message. The *canonical* code names a logical packet and travels
## inside a canonical frame `[len u16 BE][op u8][payload]`, either bare or
## tunneled under `ROOM_DATA_BYTES` as `[17]['C'][canonical frame]`.
## Mirrors `CYGWireProtocol` in the Unity SDK and `netcode.rs` on the server.

# Canonical opcodes.
const Ping := 0
const Pong := 1
const JoinRoom := 2
const LeaveRoom := 3
const PlayerInput := 4
const StateDelta := 5
const CustomRpc := 6
const RewindHit := 7
const StateSnapshot := 8
const RoomDataBytes := 9
const SpawnObject := 20
const DespawnObject := 21

# Wire codes (leading byte of a WebSocket message).
const WireJoinRoom := 10
const WireError := 11
const WireLeaveRoom := 12
const WireRoomData := 13
const WireRoomState := 14
const WireRoomStatePatch := 15
const WireRoomDataBytes := 17
const WirePing := 18
const WireRoomRequest := 21
const WireRoomResponse := 22

const CODE_MASK := 0x1F
const TUNNEL_MAGIC := 0x43  # 'C'
const HEADER_SIZE := 3
const SERIALIZER_ID := "schema"

## Leading byte of a Godot `MultiplayerPeer` packet tunneled as an RPC with
## `view_id = GODOT_PIPE_VIEW_ID` and `method_hash = GODOT_PIPE_HASH`.
const GODOT_PIPE_VIEW_ID := 0
const GODOT_PIPE_HASH := 0xFFFF
const GODOT_PIPE_TARGET := 2

# Canonical ops the engine relays as opaque room bytes (tunneled outbound).
# ROOM_DATA_BYTES is included because a relay room ignores bare ROOM_DATA (13):
# a named event only reaches peers when it travels inside the tunnel.
const TUNNELED_OPS := [CustomRpc, RewindHit, SpawnObject, DespawnObject, RoomDataBytes]

# RPC routing targets, wire-compatible with the Unity SDK's CYGRPCTarget.
enum Target { ALL, OTHERS, OBSERVERS, MASTER_CLIENT, OWNER, TARGET_PLAYER }

# Server-side target-byte aliases used by the relay (netcode.rs).
const RPC_TARGET_SERVER := 0
const RPC_TARGET_OTHERS := 1
const RPC_TARGET_ALL := 2
const RPC_TARGET_OWNER := 3

## Maps a `Target` onto the routing byte the engine relay acts on.
##
## The relay reads byte 6 of an RPC body and only distinguishes `0` (absorbed by
## the server), `3` (unicast to the view's spawner) and "anything else"
## (multicast, sender excluded). `Target.ALL` is 0 in the Unity enum, so writing
## it unmodified would have the server swallow every broadcast; OBSERVERS,
## MASTER_CLIENT and TARGET_PLAYER have no dedicated relay path and travel as a
## broadcast the receiving SDK filters.
static func server_target(target: int) -> int:
	match target:
		Target.OTHERS, Target.OBSERVERS:
			return RPC_TARGET_OTHERS
		Target.OWNER:
			return RPC_TARGET_OWNER
		_:
			return RPC_TARGET_ALL

# RFC 6455 close codes the room server uses.
const CloseConsented := 4000
const CloseServerShutdown := 4001
const CloseWithError := 4002
const CloseFailedToReconnect := 4003
const CloseMayTryReconnect := 4010

## Maps a canonical opcode onto the wire byte that carries it.
static func to_wire(op: int) -> int:
	match op:
		Ping, Pong:
			return WirePing
		JoinRoom, WireJoinRoom:
			return WireJoinRoom
		LeaveRoom, WireLeaveRoom:
			return WireLeaveRoom
		PlayerInput, WireRoomData:
			return WireRoomData
		StateDelta, WireRoomStatePatch:
			return WireRoomStatePatch
		StateSnapshot, WireRoomState:
			return WireRoomState
		_:
			return WireRoomDataBytes


## Maps an inbound wire byte onto the canonical op it normalizes to.
## Unknown codes return -1 (the message is then read as raw canonical frames).
static func from_wire(code: int) -> int:
	match code:
		WirePing:
			return Pong
		WireJoinRoom:
			return WireJoinRoom
		WireLeaveRoom:
			return WireLeaveRoom
		WireRoomData:
			return WireRoomData
		WireRoomState:
			return WireRoomState
		WireRoomStatePatch:
			return StateDelta
		WireRoomDataBytes:
			return RoomDataBytes
		WireError:
			return WireError
		WireRoomRequest:
			return WireRoomRequest
		WireRoomResponse:
			return WireRoomResponse
		_:
			return -1


static func is_canonical_op(op: int) -> bool:
	return op in [
		Ping, Pong, JoinRoom, LeaveRoom, PlayerInput, StateDelta, CustomRpc,
		RewindHit, StateSnapshot, RoomDataBytes, WireJoinRoom, WireLeaveRoom,
		WireRoomData, WireRoomState, WireRoomStatePatch, WireRoomDataBytes,
		SpawnObject, DespawnObject,
	]


static func is_tunneled(op: int) -> bool:
	return op in TUNNELED_OPS
