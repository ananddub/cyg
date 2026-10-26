// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "CYGOpCodes.generated.h"

/**
 * Opcodes carried in the third byte of every canonical CookYourGame packet.
 *
 * A canonical packet is [payload length u16 big-endian][opcode u8][payload];
 * the length covers the payload only, never the header. The values are part of
 * the wire contract with the room server and must not be renumbered.
 */
UENUM(BlueprintType)
enum class ECYGOpCode : uint8
{
	/** Heartbeat request; the payload is echoed back. */
	Ping = 0 UMETA(DisplayName = "Ping"),

	/** Heartbeat acknowledgement. */
	Pong = 1 UMETA(DisplayName = "Pong"),

	/** Logical join for the room this client is seated in. */
	JoinRoom = 2 UMETA(DisplayName = "Join Room"),

	/** Logical leave; the server answers with a socket close. */
	LeaveRoom = 3 UMETA(DisplayName = "Leave Room"),

	/** One tick of player input. */
	PlayerInput = 4 UMETA(DisplayName = "Player Input"),

	/** Synced property delta for one actor. */
	StateDelta = 5 UMETA(DisplayName = "State Delta"),

	/** Remote procedure call against a networked actor. */
	CustomRpc = 6 UMETA(DisplayName = "Rpc"),

	/** Lag-compensated hit, replayed against the shooter's timeline. */
	RewindHit = 7 UMETA(DisplayName = "Rewind Hit"),

	/** Full state of the room, sent on (re)join. */
	StateSnapshot = 8 UMETA(DisplayName = "State Snapshot"),

	/** Room handshake on the wire protocol. */
	ProtoJoinRoom = 10 UMETA(DisplayName = "Wire Join"),

	/** Room leave on the wire protocol. */
	ProtoLeaveRoom = 12 UMETA(DisplayName = "Wire Leave"),

	/** Named room event. */
	ProtoRoomData = 13 UMETA(DisplayName = "Wire Room Event"),

	/** Authoritative room state. */
	ProtoRoomState = 14 UMETA(DisplayName = "Wire Room State"),

	/** Incremental room state patch. */
	ProtoStatePatch = 15 UMETA(DisplayName = "Wire State Patch"),

	/** Opaque binary room payload. */
	ProtoDataBytes = 17 UMETA(DisplayName = "Wire Room Bytes"),

	/** Deterministic input-hub tick (PING/PONG owns 18). */
	InputTick = 19 UMETA(DisplayName = "Input Tick"),

	/** Tells every peer to instantiate a registered class. */
	SpawnObject = 20 UMETA(DisplayName = "Spawn Actor"),

	/** Tells every peer to destroy a networked actor. */
	DespawnObject = 21 UMETA(DisplayName = "Despawn Actor"),
};
