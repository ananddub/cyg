// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Core/CYGOpCodes.h"

/**
 * Translation between the canonical packet the game layer speaks and the
 * one-byte message code the room server puts on the socket.
 *
 * Outbound: [wire code u8][payload], except for the tunneled game opcodes,
 * which travel as [17]['C'][whole canonical packet] so the room server can tell
 * a CookYourGame packet apart from the msgpack, voice and delta blobs it also
 * forwards on that channel.
 */
struct FCYGWireProtocol
{
	/** [length u16 big-endian][opcode u8] */
	static constexpr int32 HeaderSize = 3;

	/** Bits that carry a message code in the first byte of a socket message. */
	static constexpr uint8 CodeMask = 0x1F;

	/** First byte inside ProtoDataBytes that marks a tunneled CookYourGame packet. */
	static constexpr uint8 TunnelMagic = 'C';

	/** Largest payload that still fits the 16-bit length header. */
	static constexpr int32 MaxPayloadSize = 0xFFFF;

	/** Largest socket message the transport accepts. */
	static constexpr int32 MaxWireMessageSize = 65536;

	static constexpr uint8 WireJoinRoom = 10;
	static constexpr uint8 WireError = 11;
	static constexpr uint8 WireLeaveRoom = 12;
	static constexpr uint8 WireRoomData = 13;
	static constexpr uint8 WireRoomState = 14;
	static constexpr uint8 WireRoomStatePatch = 15;
	static constexpr uint8 WireRoomDataBytes = 17;
	static constexpr uint8 WirePing = 18;
	static constexpr uint8 WireInputTick = 19;
	static constexpr uint8 WireRoomRequest = 21;
	static constexpr uint8 WireRoomResponse = 22;

	/** RFC 6455 codes the room server uses; 4001..4003 mean the seat is gone. */
	static constexpr int32 CloseCleanLeave = 4000;
	static constexpr int32 CloseServerShutdown = 4001;
	static constexpr int32 CloseWithError = 4002;
	static constexpr int32 CloseFailedToReconnect = 4003;
	static constexpr int32 CloseResumableSeat = 4010;

	/** True when the byte is a legal opcode in a canonical packet header. */
	static bool IsCanonicalOp(uint8 RawOp);

	/** True when the opcode leaves as a tunneled ProtoDataBytes message. */
	static bool IsTunneledOp(ECYGOpCode Op);

	/** Message code the room server expects for an opcode. */
	static uint8 ToWireCode(ECYGOpCode Op);

	/** Maps an inbound message code back onto the opcode it should surface as. */
	static bool TryFromWireCode(uint8 Code, ECYGOpCode& OutOp);

	/** True when a close code or error code kills the resume loop. */
	static bool IsTerminalClose(int32 Code);
};
