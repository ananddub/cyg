// Copyright CookYourGame. All rights reserved.

#include "Transport/CYGWebSocketTransport.h"

#include "CookYourGame.h"
#include "IWebSocket.h"
#include "WebSockets.h"

FCYGWebSocketTransport::~FCYGWebSocketTransport()
{
	Close();
}

void FCYGWebSocketTransport::Open(const FString& Url, const FString& AuthToken)
{
	Close();

	if (Url.IsEmpty())
	{
		UE_LOG(LogCookYourGame, Error, TEXT("A room url is required to open the websocket transport."));
		return;
	}

	const TSharedRef<IWebSocket, ESPMode::ThreadSafe> Created = FWebSocketsModule::Get().CreateWebSocket(Url);

	// Handshake headers are unavailable on some platforms, which is why the seat
	// token also travels in the query string.
	if (!AuthToken.IsEmpty())
	{
		TMap<FString, FString> Headers;
		Headers.Add(TEXT("Authorization"), FString::Printf(TEXT("Bearer %s"), *AuthToken));
		Created->SetHeaders(Headers);
	}

	Attach(Created);
	Socket = Created;
	Socket->Connect();
}

void FCYGWebSocketTransport::Attach(const TSharedRef<IWebSocket, ESPMode::ThreadSafe>& Created)
{
	// The socket owns these bindings and dies with them, and this transport always
	// closes the socket before it is destroyed, so the captured pointer stays live.
	Created->OnConnected().AddLambda(
		[this](auto&&... Resource)
		{
			NotifyConnected();
		});

	Created->OnMessageRaw().AddLambda(
		[this](auto&& Payload, auto&&... Text)
		{
			HandleBinaryFrame(Payload.GetData(), static_cast<int32>(Payload.Num()));
		});

	Created->OnClosed().AddLambda(
		[this](auto&&... Args)
		{
			HandleClosed(Args...);
		});

	Created->OnError().AddLambda(
		[this](auto&& Error)
		{
			HandleFailed(Error.ToString());
		});
}

void FCYGWebSocketTransport::HandleBinaryFrame(const void* Payload, int32 Size)
{
	DeliverWireMessage(static_cast<const uint8*>(Payload), Size);
}

void FCYGWebSocketTransport::HandleClosed(uint32 ReasonCode, const FString& ReasonText)
{
	// Close() already lowered the flag, so a teardown this side started stays quiet.
	if (!bConnected)
	{
		return;
	}

	FString Reason = ReasonText.IsEmpty() ? FString(TEXT("the room socket closed")) : ReasonText;
	NotifyDisconnected(Reason, static_cast<int32>(ReasonCode));
}

void FCYGWebSocketTransport::HandleFailed(const FString& Reason)
{
	if (!bConnected)
	{
		return;
	}

	const FString Detail = Reason.IsEmpty() ? FString(TEXT("the room socket failed")) : Reason;
	NotifyDisconnected(Detail, 0);
}

void FCYGWebSocketTransport::SendPacket(const uint8* Packet, int32 Size)
{
	if (Packet == nullptr || Size < FCYGWireProtocol::HeaderSize)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("Refused to send a packet shorter than the %d byte header."), FCYGWireProtocol::HeaderSize);
		return;
	}

	const uint8 RawOp = Packet[2];

	if (!FCYGWireProtocol::IsCanonicalOp(RawOp))
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("Refused to send opcode %d, which the wire protocol does not define."), RawOp);
		return;
	}

	if (!bConnected || Socket == nullptr)
	{
		UE_LOG(LogCookYourGame, Verbose, TEXT("Dropped an outbound packet for opcode %d: the socket is not open."), RawOp);
		return;
	}

	const ECYGOpCode Op = static_cast<ECYGOpCode>(RawOp);
	const bool bTunneled = FCYGWireProtocol::IsTunneledOp(Op);
	const int32 PayloadSize = Size - FCYGWireProtocol::HeaderSize;

	TArray<uint8> Wire;
	Wire.Reserve(bTunneled ? Size + 2 : PayloadSize + 1);
	Wire.Add(FCYGWireProtocol::ToWireCode(Op));

	if (bTunneled)
	{
		Wire.Add(FCYGWireProtocol::TunnelMagic);
		Wire.Append(Packet, Size);
	}
	else
	{
		Wire.Append(Packet + FCYGWireProtocol::HeaderSize, PayloadSize);
	}

	if (Wire.Num() > FCYGWireProtocol::MaxWireMessageSize)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("Packet of %d bytes is too large for one socket message; it was dropped."), Wire.Num());
		return;
	}

	Socket->Send(Wire);
}

void FCYGWebSocketTransport::Close()
{
	TSharedPtr<IWebSocket, ESPMode::ThreadSafe> Previous = Socket;
	Socket.Reset();

	// Clearing the flag first keeps the close callback from reporting a drop the
	// client asked for.
	bConnected = false;

	if (Previous.IsValid())
	{
		Previous->Close();
	}
}
