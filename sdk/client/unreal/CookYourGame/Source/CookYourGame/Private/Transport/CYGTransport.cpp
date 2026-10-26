// Copyright CookYourGame. All rights reserved.

#include "Transport/CYGTransport.h"

#include "CookYourGame.h"
#include "Containers/StringConv.h"

void FCYGTransport::NotifyConnected()
{
	bConnected = true;
	LastCloseCode = 0;
	OnConnected.Broadcast();
}

void FCYGTransport::NotifyDisconnected(const FString& Reason, int32 CloseCode)
{
	bConnected = false;
	LastCloseCode = CloseCode;
	OnDisconnected.Broadcast(Reason, CloseCode);
}

void FCYGTransport::NotifyError(const FString& Detail)
{
	OnError.Broadcast(Detail);
}

bool FCYGTransport::CountCanonicalPackets(const uint8* Data, int32 Start, int32 Size, int32& OutPackets)
{
	OutPackets = 0;
	const int32 End = Start + Size;
	int32 Cursor = Start;

	while (Cursor < End)
	{
		if (End - Cursor < FCYGWireProtocol::HeaderSize)
		{
			return false;
		}

		const int32 Payload = (Data[Cursor] << 8) | Data[Cursor + 1];
		const int32 Total = FCYGWireProtocol::HeaderSize + Payload;

		if (Cursor + Total > End || !FCYGWireProtocol::IsCanonicalOp(Data[Cursor + 2]))
		{
			return false;
		}

		Cursor += Total;
		++OutPackets;
	}

	return OutPackets > 0;
}

void FCYGTransport::DeliverNormalizedPacket(ECYGOpCode Op, const uint8* Payload, int32 Size)
{
	TArray<uint8> Packet;
	Packet.Reserve(FCYGWireProtocol::HeaderSize + Size);
	Packet.Add(static_cast<uint8>(Size >> 8));
	Packet.Add(static_cast<uint8>(Size & 0xFF));
	Packet.Add(static_cast<uint8>(Op));
	Packet.Append(Payload, Size);

	OnPacket.Broadcast(Packet.GetData(), Packet.Num());
}

void FCYGTransport::DeliverWireMessage(const uint8* Data, int32 Size)
{
	if (Data == nullptr || Size <= 0)
	{
		return;
	}

	if (Size > FCYGWireProtocol::MaxWireMessageSize)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("Ignored a %d byte socket message above the transport limit."), Size);
		return;
	}

	const uint8 Leading = Data[0] & FCYGWireProtocol::CodeMask;
	int32 Packets = 0;

	// Game traffic rides ProtoDataBytes as [17]['C'][packet][packet]...
	if (Leading == FCYGWireProtocol::WireRoomDataBytes
		&& Size > 2
		&& Data[1] == FCYGWireProtocol::TunnelMagic
		&& CountCanonicalPackets(Data, 2, Size - 2, Packets))
	{
		int32 Cursor = 2;

		for (int32 Index = 0; Index < Packets; ++Index)
		{
			const int32 Total = FCYGWireProtocol::HeaderSize + ((Data[Cursor] << 8) | Data[Cursor + 1]);
			OnPacket.Broadcast(Data + Cursor, Total);
			Cursor += Total;
		}

		return;
	}

	// A rejection arrives as wire code 11 with the server's reason as UTF-8 text.
	if (Leading == FCYGWireProtocol::WireError)
	{
		const FString Reason = Size > 1
			? FString(FUTF8ToTCHAR(reinterpret_cast<const ANSICHAR*>(Data + 1), Size - 1).Get())
			: FString(TEXT("the server rejected the connection"));

		NotifyError(Reason);
		return;
	}

	// Room messages carry the code in byte zero and let the socket supply the
	// length, so the canonical header is rebuilt here.
	ECYGOpCode Op = ECYGOpCode::Ping;

	if (FCYGWireProtocol::TryFromWireCode(Leading, Op))
	{
		DeliverNormalizedPacket(Op, Data + 1, Size - 1);
		return;
	}

	if (CountCanonicalPackets(Data, 0, Size, Packets))
	{
		int32 Cursor = 0;

		for (int32 Index = 0; Index < Packets; ++Index)
		{
			const int32 Total = FCYGWireProtocol::HeaderSize + ((Data[Cursor] << 8) | Data[Cursor + 1]);
			OnPacket.Broadcast(Data + Cursor, Total);
			Cursor += Total;
		}

		return;
	}

	UE_LOG(LogCookYourGame, Warning, TEXT("Could not deframe a %d byte message starting with code %d."), Size, Leading);
}
