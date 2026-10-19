// Copyright CookYourGame. All rights reserved.

#include "Transport/CYGProfilingTransport.h"

#include "Profiler/CYGNetworkProfiler.h"

FCYGProfilingTransport::FCYGProfilingTransport(const TSharedPtr<FCYGTransport>& InInner)
	: Inner(InInner)
{
	if (!Inner.IsValid())
	{
		return;
	}

	Inner->OnConnected.AddLambda(
		[this]()
		{
			bConnected = true;
			OnConnected.Broadcast();
		});

	Inner->OnDisconnected.AddLambda(
		[this](const FString& Reason, int32 CloseCode)
		{
			bConnected = false;
			LastCloseCode = CloseCode;
			OnDisconnected.Broadcast(Reason, CloseCode);
		});

	Inner->OnPacket.AddLambda(
		[this](const uint8* Packet, int32 Size)
		{
			HandlePacket(Packet, Size);
		});

	Inner->OnError.AddLambda(
		[this](const FString& Detail)
		{
			OnError.Broadcast(Detail);
		});
}

void FCYGProfilingTransport::Open(const FString& Url, const FString& AuthToken)
{
	if (Inner.IsValid())
	{
		Inner->Open(Url, AuthToken);
	}
}

void FCYGProfilingTransport::SendPacket(const uint8* Packet, int32 Size)
{
	ECYGOpCode Op = ECYGOpCode::Ping;
	int32 PayloadBytes = 0;

	if (FCYGNetworkProfiler::bEnabled && TryReadHeader(Packet, Size, Op, PayloadBytes))
	{
		FCYGNetworkProfiler::RecordSend(Op, PayloadBytes);
	}

	if (Inner.IsValid())
	{
		Inner->SendPacket(Packet, Size);
	}
}

void FCYGProfilingTransport::Close()
{
	if (Inner.IsValid())
	{
		Inner->Close();
	}
}

void FCYGProfilingTransport::Tick(float DeltaSeconds)
{
	if (Inner.IsValid())
	{
		Inner->Tick(DeltaSeconds);

		// The client reads connection state off this wrapper, so it has to mirror the
		// link it sits on rather than the moment Open was called.
		bConnected = Inner->IsConnected();
		LastCloseCode = Inner->GetLastCloseCode();
	}
}

void FCYGProfilingTransport::HandlePacket(const uint8* Packet, int32 Size)
{
	ECYGOpCode Op = ECYGOpCode::Ping;
	int32 PayloadBytes = 0;

	if (FCYGNetworkProfiler::bEnabled && TryReadHeader(Packet, Size, Op, PayloadBytes))
	{
		FCYGNetworkProfiler::RecordReceive(Op, PayloadBytes);
	}

	OnPacket.Broadcast(Packet, Size);
}

bool FCYGProfilingTransport::TryReadHeader(const uint8* Packet, int32 Size, ECYGOpCode& OutOp, int32& OutPayloadBytes)
{
	if (Packet == nullptr || Size < FCYGWireProtocol::HeaderSize)
	{
		return false;
	}

	// [length u16 big-endian][opcode u8][payload]
	const uint8 RawOp = Packet[2];

	if (!FCYGWireProtocol::IsCanonicalOp(RawOp))
	{
		return false;
	}

	OutOp = static_cast<ECYGOpCode>(RawOp);
	OutPayloadBytes = Size - FCYGWireProtocol::HeaderSize;

	return OutPayloadBytes >= 0;
}
