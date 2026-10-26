// Copyright CookYourGame. All rights reserved.

#include "Transport/CYGWireProtocol.h"

bool FCYGWireProtocol::IsCanonicalOp(uint8 RawOp)
{
	switch (RawOp)
	{
	case static_cast<uint8>(ECYGOpCode::Ping):
	case static_cast<uint8>(ECYGOpCode::Pong):
	case static_cast<uint8>(ECYGOpCode::JoinRoom):
	case static_cast<uint8>(ECYGOpCode::LeaveRoom):
	case static_cast<uint8>(ECYGOpCode::PlayerInput):
	case static_cast<uint8>(ECYGOpCode::StateDelta):
	case static_cast<uint8>(ECYGOpCode::CustomRpc):
	case static_cast<uint8>(ECYGOpCode::RewindHit):
	case static_cast<uint8>(ECYGOpCode::StateSnapshot):
	case static_cast<uint8>(ECYGOpCode::ProtoJoinRoom):
	case static_cast<uint8>(ECYGOpCode::ProtoLeaveRoom):
	case static_cast<uint8>(ECYGOpCode::ProtoRoomData):
	case static_cast<uint8>(ECYGOpCode::ProtoRoomState):
	case static_cast<uint8>(ECYGOpCode::ProtoStatePatch):
	case static_cast<uint8>(ECYGOpCode::ProtoDataBytes):
	case static_cast<uint8>(ECYGOpCode::InputTick):
	case static_cast<uint8>(ECYGOpCode::SpawnObject):
	case static_cast<uint8>(ECYGOpCode::DespawnObject):
		return true;
	default:
		return false;
	}
}

bool FCYGWireProtocol::IsTunneledOp(ECYGOpCode Op)
{
	return Op == ECYGOpCode::CustomRpc
		|| Op == ECYGOpCode::RewindHit
		|| Op == ECYGOpCode::SpawnObject
		|| Op == ECYGOpCode::DespawnObject;
}

uint8 FCYGWireProtocol::ToWireCode(ECYGOpCode Op)
{
	switch (Op)
	{
	case ECYGOpCode::Ping:
	case ECYGOpCode::Pong:
		return WirePing;
	case ECYGOpCode::JoinRoom:
	case ECYGOpCode::ProtoJoinRoom:
		return WireJoinRoom;
	case ECYGOpCode::LeaveRoom:
	case ECYGOpCode::ProtoLeaveRoom:
		return WireLeaveRoom;
	case ECYGOpCode::PlayerInput:
	case ECYGOpCode::ProtoRoomData:
		return WireRoomData;
	case ECYGOpCode::StateDelta:
	case ECYGOpCode::ProtoStatePatch:
		return WireRoomStatePatch;
	case ECYGOpCode::StateSnapshot:
	case ECYGOpCode::ProtoRoomState:
		return WireRoomState;
	default:
		return WireRoomDataBytes;
	}
}

bool FCYGWireProtocol::TryFromWireCode(uint8 Code, ECYGOpCode& OutOp)
{
	switch (Code)
	{
	case WirePing:
		OutOp = ECYGOpCode::Pong;
		return true;
	case WireJoinRoom:
		OutOp = ECYGOpCode::ProtoJoinRoom;
		return true;
	case WireLeaveRoom:
		OutOp = ECYGOpCode::ProtoLeaveRoom;
		return true;
	case WireRoomData:
		OutOp = ECYGOpCode::ProtoRoomData;
		return true;
	case WireRoomState:
		OutOp = ECYGOpCode::ProtoRoomState;
		return true;
	case WireRoomStatePatch:
		OutOp = ECYGOpCode::ProtoStatePatch;
		return true;
	case WireRoomDataBytes:
		OutOp = ECYGOpCode::ProtoDataBytes;
		return true;
	case WireInputTick:
		OutOp = ECYGOpCode::InputTick;
		return true;
	default:
		OutOp = ECYGOpCode::Ping;
		return false;
	}
}

bool FCYGWireProtocol::IsTerminalClose(int32 Code)
{
	return Code == CloseServerShutdown || Code == CloseWithError || Code == CloseFailedToReconnect;
}
