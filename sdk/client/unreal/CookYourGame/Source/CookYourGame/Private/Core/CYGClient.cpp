// Copyright CookYourGame. All rights reserved.

#include "Core/CYGClient.h"

#include "CookYourGame.h"
#include "Core/CYGSettings.h"
#include "Containers/StringConv.h"
#include "Dom/JsonObject.h"
#include "Gameplay/CYGCoordinateSpace.h"
#include "HAL/PlatformProcess.h"
#include "HAL/PlatformTime.h"
#include "HttpModule.h"
#include "Interfaces/IHttpRequest.h"
#include "Interfaces/IHttpResponse.h"
#include "Profiler/CYGNetworkProfiler.h"
#include "Serialization/CYGBitPacker.h"
#include "Serialization/JsonReader.h"
#include "Resilience/CYGSessionCookie.h"
#include "Serialization/JsonSerializer.h"
#include "Transport/CYGProfilingTransport.h"
#include "Transport/CYGWebSocketTransport.h"

namespace
{
	const TCHAR* EventPlayerJoin = TEXT("cyg_player_join");
	const TCHAR* EventPlayerLeave = TEXT("cyg_player_leave");
	const TCHAR* SerializerId = TEXT("schema");

	constexpr int32 MaxEventNameBytes = 96;
	constexpr int32 WirePayloadLimit = 0xFFFF;

	FString ReadTextField(const TSharedPtr<FJsonObject>& Object, const TCHAR* Field)
	{
		if (!Object.IsValid())
		{
			return FString();
		}

		const TSharedPtr<FJsonValue>* Value = Object->TryGetField(Field);

		if (Value == nullptr || !(*Value).IsValid())
		{
			return FString();
		}

		return (*Value)->AsString();
	}

	int32 ReadNumberField(const TSharedPtr<FJsonObject>& Object, const TCHAR* Field)
	{
		if (!Object.IsValid())
		{
			return 0;
		}

		const TSharedPtr<FJsonValue>* Value = Object->TryGetField(Field);

		if (Value == nullptr || !(*Value).IsValid())
		{
			return 0;
		}

		return FMath::RoundToInt32(static_cast<float>((*Value)->AsNumber()));
	}

	TSharedPtr<FJsonObject> ParseJson(const FString& Text)
	{
		TSharedPtr<FJsonObject> Object;
		const TSharedRef<TJsonReader<>> Reader = TJsonReaderFactory<>::Create(Text);

		if (!FJsonSerializer::Deserialize(Reader, Object) || !Object.IsValid())
		{
			return nullptr;
		}

		return Object;
	}

	FString EscapeQuoted(const FString& Value)
	{
		FString Escaped;
		Escaped.Reserve(Value.Len() + 8);

		for (int32 Index = 0; Index < Value.Len(); ++Index)
		{
			switch (Value[Index])
			{
			case TEXT('"'): Escaped += TEXT("\\\""); break;
			case TEXT('\\'): Escaped += TEXT("\\\\"); break;
			case TEXT('\n'): Escaped += TEXT("\\n"); break;
			case TEXT('\r'): Escaped += TEXT("\\r"); break;
			case TEXT('\t'): Escaped += TEXT("\\t"); break;
			default:
				if (Value[Index] < TEXT(' '))
				{
					Escaped += FString::Printf(TEXT("\\u%04x"), static_cast<uint16>(Value[Index]));
				}
				else
				{
					Escaped += Value[Index];
				}

				break;
			}
		}

		return Escaped;
	}

	/** Snapshot of a payload for listeners that keep it past the callback. */
	TArray<uint8> CopyOfBytes(const uint8* Data, int32 Size)
	{
		TArray<uint8> Copy;

		if (Data != nullptr && Size > 0)
		{
			Copy.Append(Data, Size);
		}

		return Copy;
	}

	FString CopyOf(const uint8* Data, int32 Size)
	{
		if (Data == nullptr || Size <= 0)
		{
			return FString();
		}

		return FString(FUTF8ToTCHAR(reinterpret_cast<const ANSICHAR*>(Data), Size).Get());
	}
}

void UCYGClient::Initialize()
{
	if (Settings == nullptr)
	{
		Settings = GetMutableDefault<UCYGSettings>();
	}

	// Counting is off unless the project asked for it, or somebody turned the sink on
	// before the first connection.
	if (Configured().bProfileTraffic)
	{
		FCYGNetworkProfiler::bEnabled = true;
	}
}

const UCYGSettings& UCYGClient::Configured() const
{
	return Settings != nullptr ? *Settings : *GetDefault<UCYGSettings>();
}

void UCYGClient::UseTransport(const TSharedPtr<FCYGTransport>& InTransport)
{
	if (IsLinkConnected())
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("A transport can only be replaced while the link is down."));
		return;
	}

	Transport = InTransport;

	if (Transport.IsValid())
	{
		BindTransportHandlers();
	}
}

void UCYGClient::BindTransportHandlers()
{
	// Substituted transports arrive already built, so the bindings that make a link
	// reach this client have to happen on the seam as well as in EnsureTransport:
	// otherwise an injected transport would be heard by nobody.
	Transport->OnConnected.AddWeakLambda(this, [this]() { HandleSocketConnected(); });

	Transport->OnDisconnected.AddWeakLambda(
		this,
		[this](const FString& Reason, int32 CloseCode) { HandleSocketLost(Reason, CloseCode); });

	Transport->OnPacket.AddWeakLambda(
		this,
		[this](const uint8* Packet, int32 Size) { HandlePacket(Packet, Size); });

	Transport->OnError.AddWeakLambda(
		this,
		[this](const FString& Detail) { HandleSocketError(Detail); });
}

int32 UCYGClient::GetLastCloseCode() const
{
	return Transport.IsValid() ? Transport->GetLastCloseCode() : 0;
}

void UCYGClient::EnsureTransport()
{
	if (Transport.IsValid())
	{
		return;
	}

	if (FCYGNetworkProfiler::bEnabled)
	{
		// The wrapper forwards and re-raises everything, so counting the packets that
		// go by costs the client nothing it would not have done anyway.
		Transport = MakeShared<FCYGProfilingTransport>(MakeShared<FCYGWebSocketTransport>());
	}
	else
	{
		Transport = MakeShared<FCYGWebSocketTransport>();
	}

	BindTransportHandlers();
}

bool UCYGClient::IsLinkConnected() const
{
	return Transport.IsValid() && Transport->IsConnected();
}

void UCYGClient::Connect(const FString& SocketUrl, const FString& PlayerName)
{
	if (SocketUrl.IsEmpty())
	{
		UE_LOG(LogCookYourGame, Error, TEXT("Connect needs a room url."));
		return;
	}

	if (!SocketUrl.StartsWith(TEXT("ws://"), ESearchCase::IgnoreCase)
		&& !SocketUrl.StartsWith(TEXT("wss://"), ESearchCase::IgnoreCase))
	{
		UE_LOG(LogCookYourGame, Error, TEXT("Connect expects a ws:// or wss:// room url, received \"%s\"."), *SocketUrl);
		return;
	}

	PendingNickname = PlayerName.IsEmpty() ? FPlatformProcess::UserName(false) : PlayerName;
	PendingRoomName = UCYGSettings::NormalizeRoomName(RoomFromUrl(SocketUrl));

	Initialize();
	EnsureTransport();
	Transport->Open(SocketUrl, SeatToken);
}

void UCYGClient::ConnectToMatch(const FString& MatchId, const FString& Token, const FString& Host, int32 Port, const FString& PlayerName)
{
	if (IsLinkConnected() || SeatRequest.IsValid())
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("Already connected or reserving a seat; this attempt was ignored."));
		return;
	}

	Initialize();

	PendingRoomName = MatchId;
	PendingNickname = PlayerName.IsEmpty() ? FPlatformProcess::UserName(false) : PlayerName;

	SeatRoomId = MatchId;
	SeatSessionId = FGuid::NewGuid().ToString(EGuidFormats::Digits);
	SeatToken = Token;
	SeatProcessId = TEXT("proc_1");
	SeatGamePort = Port;

	const FString Scheme = Host.StartsWith(TEXT("wss://"), ESearchCase::IgnoreCase) ? TEXT("wss://") : TEXT("ws://");
	const FString Authority = Host.Replace(TEXT("wss://"), TEXT(""), ESearchCase::IgnoreCase).Replace(TEXT("ws://"), TEXT(""), ESearchCase::IgnoreCase);

	FString Url = Scheme + Authority + FString::Printf(TEXT(":%d"), Port);
	Url += UCYGSettings::LeadingPath(Configured().ServerWsPath);
	Url += TEXT("/?room=");
	Url += UCYGSettings::EscapeSegment(MatchId);
	Url += TEXT("&session=");
	Url += UCYGSettings::EscapeSegment(SeatSessionId);
	Url += TEXT("&token=");
	Url += UCYGSettings::EscapeSegment(Token);
	Url += TEXT("&cyg=true");

	CaptureSeatCookie();
	EnsureTransport();
	Transport->Open(Url, Token);
}

bool UCYGClient::ReconnectToSeat()
{
	if (IsLinkConnected() || SeatRequest.IsValid() || SeatSessionId.IsEmpty() || SeatToken.IsEmpty())
	{
		return false;
	}

	const FString Url = Configured().BuildGameSocketUrl(SeatProcessId, SeatRoomId, SeatSessionId, SeatToken, SeatGamePort);

	if (Url.IsEmpty())
	{
		return false;
	}

	EnsureTransport();
	Transport->Open(Url, SeatToken);
	return true;
}

void UCYGClient::CaptureSeatCookie()
{
	FCYGSessionCookie::Capture(SeatRoomId.IsEmpty() ? PendingRoomName : SeatRoomId, SeatSessionId);
}

void UCYGClient::ConnectToRoom(const FString& Room, const FString& PlayerName)
{
	ConnectToRoomAs(Room, FString(), PlayerName);
}

void UCYGClient::ConnectToRoomAs(const FString& Room, const FString& Method, const FString& PlayerName)
{
	if (IsLinkConnected() || SeatRequest.IsValid())
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("Already connected or reserving a seat; this attempt was ignored."));
		return;
	}

	Initialize();
	ReserveSeat(Room, PlayerName, Method);
}

FString UCYGClient::RoomFromUrl(const FString& SocketUrl)
{
	// The room server keys a session on the last path segment of the upgrade url.
	FString Path = SocketUrl;
	int32 Query = INDEX_NONE;

	if (Path.FindChar(TEXT('?'), Query))
	{
		Path = Path.Left(Query);
	}

	int32 Slash = INDEX_NONE;

	if (Path.FindChar(TEXT('/'), Slash, ESearchCase::Sensitive, ESearchDir::FromEnd) && Slash + 1 < Path.Len())
	{
		return Path.Mid(Slash + 1);
	}

	return FString();
}

void UCYGClient::ReserveSeat(const FString& Room, const FString& PlayerName, const FString& Method)
{
	const UCYGSettings& Active = Configured();
	PendingRoomName = UCYGSettings::NormalizeRoomName(Room);
	PendingNickname = PlayerName.IsEmpty() ? FPlatformProcess::UserName(false) : PlayerName;

	const FString Url = Active.BuildMatchmakeUrl(Method.IsEmpty() ? Active.MatchmakeMethod : Method, PendingRoomName);

	if (Url.IsEmpty())
	{
		return;
	}

	FString Body = TEXT("{\"playerName\":\"");
	Body += EscapeQuoted(PendingNickname);
	Body += FString::Printf(TEXT("\",\"maxPlayers\":%d,\"protocolVersion\":%d}"), Active.MaxPlayers, Active.ProtocolVersion);

	const TSharedRef<IHttpRequest, ESPMode::ThreadSafe> Request = FHttpModule::Get().CreateRequest();
	Request->SetURL(Url);
	Request->SetVerb(TEXT("POST"));
	Request->SetHeader(TEXT("Content-Type"), TEXT("application/json"));
	Request->SetTimeout(Active.RequestTimeoutSeconds);
	Request->SetContentAsString(Body);

	Request->OnProcessRequestComplete().AddWeakLambda(
		this,
		[this](const FHttpRequestPtr&, const FHttpResponsePtr& Response, bool bConnectedSuccessfully)
		{
			const int32 Code = Response.IsValid() ? Response->GetResponseCode() : 0;
			const FString Content = Response.IsValid() ? Response->GetContentAsString() : FString();
			OnSeatReply(bConnectedSuccessfully, Code, Content);
		});

	SeatRequest = Request;
	Request->ProcessRequest();
}

void UCYGClient::AbandonSeatRequest()
{
	if (SeatRequest.IsValid())
	{
		SeatRequest->CancelRequest();
		SeatRequest.Reset();
	}
}

void UCYGClient::OnSeatReply(bool bSucceeded, int32 ResponseCode, const FString& Content)
{
	SeatRequest.Reset();

	if (!bSucceeded)
	{
		UE_LOG(LogCookYourGame, Error, TEXT("Matchmake failed (%d): %s"), ResponseCode, *Content);
		ClearRoomState();
		OnDisconnected.Broadcast(Content.IsEmpty() ? FString(TEXT("matchmake failed")) : Content, 0);
		return;
	}

	const TSharedPtr<FJsonObject> Object = ParseJson(Content);

	if (!Object.IsValid())
	{
		UE_LOG(LogCookYourGame, Error, TEXT("Matchmake replied with something that is not an object: %s"), *Content);
		return;
	}

	SeatSessionId = ReadTextField(Object, TEXT("sessionId"));
	SeatRoomId = ReadTextField(Object, TEXT("roomId"));
	SeatProcessId = ReadTextField(Object, TEXT("processId"));
	SeatToken = ReadTextField(Object, TEXT("reconnectionToken"));

	if (SeatToken.IsEmpty())
	{
		SeatToken = ReadTextField(Object, TEXT("seatToken"));
	}

	SeatGamePort = ReadNumberField(Object, TEXT("gamePort"));

	if (SeatSessionId.IsEmpty())
	{
		UE_LOG(LogCookYourGame, Error, TEXT("Matchmake reply carried no sessionId: %s"), *Content);
		return;
	}

	const FString Url = Configured().BuildGameSocketUrl(SeatProcessId, SeatRoomId, SeatSessionId, SeatToken, SeatGamePort);

	if (Url.IsEmpty())
	{
		return;
	}

	CaptureSeatCookie();
	EnsureTransport();
	Transport->Open(Url, SeatToken);
}

void UCYGClient::Disconnect()
{
	if (IsLinkConnected())
	{
		SendFrame(ECYGOpCode::LeaveRoom, nullptr, 0);
		Transport->Close();
	}

	AbandonSeatRequest();
	FCYGSessionCookie::Clear();
	ClearRoomState();
}

void UCYGClient::ClearRoomState()
{
	bHandshakeComplete = false;
	LocalPlayer = FCYGPlayer();
	Players.Reset();
	ViewCounter = 0;
	LatencyMilliseconds = 0.f;
	LastPingAtSeconds = 0.0;
}

void UCYGClient::Tick(float DeltaSeconds)
{
	if (!Transport.IsValid())
	{
		return;
	}

	Transport->Tick(DeltaSeconds);

	if (!IsLinkConnected())
	{
		return;
	}

	const double Now = FPlatformTime::Seconds();
	const double Interval = Configured().GetPingInterval();

	if (Interval <= 0.0)
	{
		return;
	}

	if (Now - LastPingAtSeconds < Interval)
	{
		return;
	}

	LastPingAtSeconds = Now;
	PingSentAtSeconds = Now;

	FCYGBitWriter Body;
	Body.WriteUInt32BE(static_cast<uint32>(Now * 1000.0));
	SendFrame(ECYGOpCode::Ping, Body.Data().GetData(), Body.Num());
}

void UCYGClient::HandleSocketConnected()
{
	bHandshakeComplete = false;
	LastErrorCode = 0;
	LastErrorMessage.Reset();
	LastPingAtSeconds = FPlatformTime::Seconds();
	PingSentAtSeconds = LastPingAtSeconds;

	FCYGBitWriter Body;
	WriteJoinHandshake(Body, SeatToken);
	SendFrame(ECYGOpCode::ProtoJoinRoom, Body.Data().GetData(), Body.Num());
}

void UCYGClient::WriteJoinHandshake(FCYGBitWriter& Target, const FString& Token) const
{
	const FString Limited = Token.Len() > 255 ? Token.Left(255) : Token;
	const FTCHARToUTF8 Converted(*Limited);

	Target.WriteUInt8(static_cast<uint8>(Converted.Length()));
	Target.WriteRaw(reinterpret_cast<const uint8*>(Converted.Get()), Converted.Length());

	int32 SerializerLength = 0;

	while (SerializerId[SerializerLength] != TEXT('\0'))
	{
		++SerializerLength;
	}

	Target.WriteUInt8(static_cast<uint8>(SerializerLength));

	for (const TCHAR* Cursor = SerializerId; *Cursor != TEXT('\0'); ++Cursor)
	{
		Target.WriteUInt8(static_cast<uint8>(*Cursor));
	}

	Target.WriteUInt8(0);
}

void UCYGClient::HandleSocketLost(const FString& Reason, int32 CloseCode)
{
	const bool bTerminal = FCYGWireProtocol::IsTerminalClose(CloseCode) || FCYGWireProtocol::IsTerminalClose(LastErrorCode);

	if (bTerminal)
	{
		// The seat is gone for good, so a full room or a shutting-down node must not
		// be retried with the same token.
		if (CloseCode == FCYGWireProtocol::CloseWithError && LastErrorCode == 0)
		{
			ReportError(CloseCode, Reason);
		}
	}

	ClearRoomState();
	OnDisconnected.Broadcast(Reason, CloseCode);
}

void UCYGClient::HandleSocketError(const FString& Detail)
{
	const TSharedPtr<FJsonObject> Object = ParseJson(Detail);

	if (Object.IsValid())
	{
		ReportError(ReadNumberField(Object, TEXT("code")), ReadTextField(Object, TEXT("message")));
		return;
	}

	ReportError(0, Detail.IsEmpty() ? FString(TEXT("room error")) : Detail);
}

void UCYGClient::ReportError(int32 Code, const FString& Message)
{
	LastErrorCode = Code;
	LastErrorMessage = Message;

	UE_LOG(LogCookYourGame, Error, TEXT("The room server rejected the link (%d): %s"), Code, *Message);
	OnError.Broadcast(Code, Message);
}

bool UCYGClient::SendFrame(ECYGOpCode Op, const uint8* Payload, int32 Size)
{
	if (!Transport.IsValid() || !Transport->IsConnected())
	{
		UE_LOG(LogCookYourGame, Verbose, TEXT("A %d opcode was dropped: the link is down."), static_cast<int32>(Op));
		return false;
	}

	if (Size < 0 || Size > WirePayloadLimit)
	{
		UE_LOG(LogCookYourGame, Error, TEXT("A payload of %d bytes does not fit the 16-bit length header."), Size);
		return false;
	}

	TArray<uint8> Packet;
	Packet.Reserve(FCYGWireProtocol::HeaderSize + Size);
	Packet.Add(static_cast<uint8>(Size >> 8));
	Packet.Add(static_cast<uint8>(Size & 0xFF));
	Packet.Add(static_cast<uint8>(Op));

	if (Size > 0)
	{
		Packet.Append(Payload, Size);
	}

	Transport->SendPacket(Packet.GetData(), Packet.Num());
	return true;
}

bool UCYGClient::SendPacket(ECYGOpCode Op, const TArray<uint8>& Payload)
{
	return SendFrame(Op, Payload.GetData(), Payload.Num());
}

bool UCYGClient::SendInputPayload(const TArray<uint8>& Payload)
{
	return SendFrame(ECYGOpCode::PlayerInput, Payload.GetData(), Payload.Num());
}

bool UCYGClient::SendViewDelta(int32 ViewId, const TArray<uint8>& Payload)
{
	FCYGBitWriter Body;
	Body.WriteInt32(ViewId);
	Body.WriteRaw(Payload);
	return SendFrame(ECYGOpCode::StateDelta, Body.Data().GetData(), Body.Num());
}

bool UCYGClient::SendRpcPayload(const TArray<uint8>& Payload)
{
	return SendFrame(ECYGOpCode::CustomRpc, Payload.GetData(), Payload.Num());
}

bool UCYGClient::SendRoomEvent(const FString& EventName, const TArray<uint8>& Body)
{
	if (EventName.IsEmpty())
	{
		UE_LOG(LogCookYourGame, Error, TEXT("A room event needs a name."));
		return false;
	}

	FCYGBitWriter Packet;
	Packet.WriteString(EventName);
	Packet.WriteRaw(Body);
	return SendFrame(ECYGOpCode::ProtoRoomData, Packet.Data().GetData(), Packet.Num());
}

bool UCYGClient::SendSpawnRequest(int32 ViewId, uint16 PrefabId, const FVector& Location, const FQuat& Rotation, const TArray<uint8>& Arguments)
{
	FCYGBitWriter Packet;
	Packet.WriteInt32(ViewId);
	Packet.WriteUInt16BE(PrefabId);
	Packet.WriteUInt32(static_cast<uint32>(FMath::Max(0, LocalPlayer.ActorNumber)));
	Packet.WriteVector3(FCYGCoordinateSpace::ToWire(Location));
	Packet.WriteQuaternionRaw(FCYGCoordinateSpace::ToWire(Rotation));
	Packet.WriteRaw(Arguments);
	return SendFrame(ECYGOpCode::SpawnObject, Packet.Data().GetData(), Packet.Num());
}

bool UCYGClient::SendDespawnRequest(int32 ViewId)
{
	FCYGBitWriter Packet;
	Packet.WriteInt32(ViewId);
	return SendFrame(ECYGOpCode::DespawnObject, Packet.Data().GetData(), Packet.Num());
}

int32 UCYGClient::NextViewIdForOwner(int32 ActorNumber)
{
	ViewCounter = (ViewCounter + 1) & 0xFFFF;
	return (FMath::Max(0, ActorNumber) << 16) | ViewCounter;
}

void UCYGClient::HandlePacket(const uint8* Packet, int32 Size)
{
	if (Packet == nullptr || Size < FCYGWireProtocol::HeaderSize)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("Received a runt frame of %d bytes."), Size);
		return;
	}

	const int32 PayloadSize = (Packet[0] << 8) | Packet[1];

	if (FCYGWireProtocol::HeaderSize + PayloadSize > Size)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("A frame header claimed %d payload bytes but only %d arrived."), PayloadSize, Size - FCYGWireProtocol::HeaderSize);
		return;
	}

	Route(static_cast<ECYGOpCode>(Packet[2]), Packet + FCYGWireProtocol::HeaderSize, PayloadSize);
}

void UCYGClient::Route(ECYGOpCode Op, const uint8* Payload, int32 Size)
{
	switch (Op)
	{
	case ECYGOpCode::Pong:
		LatencyMilliseconds = static_cast<float>((FPlatformTime::Seconds() - PingSentAtSeconds) * 1000.0);
		break;

	case ECYGOpCode::Ping:
		break;

	case ECYGOpCode::ProtoJoinRoom:
		HandleJoinHandshake(Payload, Size);
		break;

	case ECYGOpCode::ProtoRoomState:
	case ECYGOpCode::StateSnapshot:
		OnRoomStateNative.Broadcast(Payload, Size);
		OnRoomStateReceived.Broadcast(CopyOfBytes(Payload, Size));
		break;

	case ECYGOpCode::ProtoRoomData:
		ForwardRoomEvent(Payload, Size);
		break;

	case ECYGOpCode::ProtoStatePatch:
	case ECYGOpCode::StateDelta:
		HandleViewDelta(Payload, Size);
		break;

	case ECYGOpCode::CustomRpc:
		HandleRpc(Payload, Size);
		break;

	case ECYGOpCode::SpawnObject:
		HandleSpawn(Payload, Size);
		break;

	case ECYGOpCode::DespawnObject:
		HandleDespawn(Payload, Size);
		break;

	case ECYGOpCode::RewindHit:
		OnRewindHitNative.Broadcast(Payload, Size);
		break;

	case ECYGOpCode::ProtoDataBytes:
		HandleRoomBytes(Payload, Size);
		break;

	case ECYGOpCode::ProtoLeaveRoom:
		// The room answers a leave with a socket close, so this only arrives from a
		// host that speaks the game protocol itself.
		BroadcastRoomBytes(Payload, Size);
		break;

	default:
		UE_LOG(LogCookYourGame, Warning, TEXT("No route for opcode %d."), static_cast<int32>(Op));
		break;
	}
}

void UCYGClient::BroadcastRoomBytes(const uint8* Payload, int32 Size)
{
	OnRoomBytes.Broadcast(CopyOfBytes(Payload, Size));
}

void UCYGClient::HandleJoinHandshake(const uint8* Payload, int32 Size)
{
	FCYGBitReader Reader(Payload, Size);

	FString ReconnectionToken;
	FString Serializer;

	if (Reader.Remaining() > 0)
	{
		ReconnectionToken = Reader.ReadFixedText(Reader.ReadUInt8());
	}

	if (Reader.Remaining() > 0)
	{
		Serializer = Reader.ReadFixedText(Reader.ReadUInt8());
	}

	int32 Colon = INDEX_NONE;

	if (ReconnectionToken.FindChar(TEXT(':'), Colon))
	{
		SeatRoomId = ReconnectionToken.Left(Colon);
		SeatSessionId = ReconnectionToken.Mid(Colon + 1);
	}

	if (!Serializer.IsEmpty() && Serializer != SerializerId)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("The room serializes with \"%s\", which this client does not decode."), *Serializer);
	}

	RegisterLocalPlayer();
	bHandshakeComplete = true;

	OnConnected.Broadcast();
	OnJoinedRoom.Broadcast(GetRoomName());
}

void UCYGClient::ForwardRoomEvent(const uint8* Payload, int32 Size)
{
	FString EventName;
	int32 BodyOffset = 0;
	int32 BodySize = 0;

	if (!TrySplitEventName(Payload, Size, EventName, BodyOffset, BodySize))
	{
		BroadcastRoomBytes(Payload, Size);
		return;
	}

	const uint8* Body = Payload + BodyOffset;

	TrackRosterEvent(EventName, Body, BodySize);
	OnRoomEvent.Broadcast(EventName, CopyOfBytes(Body, BodySize));
}

void UCYGClient::HandleRoomBytes(const uint8* Payload, int32 Size)
{
	FString EventName;
	int32 BodyOffset = 0;
	int32 BodySize = 0;

	if (TrySplitEventName(Payload, Size, EventName, BodyOffset, BodySize))
	{
		ForwardRoomEvent(Payload, Size);
		return;
	}

	BroadcastRoomBytes(Payload, Size);
}

bool UCYGClient::TrySplitEventName(const uint8* Data, int32 Size, FString& OutName, int32& OutBodyOffset, int32& OutBodySize)
{
	OutName.Reset();
	OutBodyOffset = 0;
	OutBodySize = Size;

	if (Data == nullptr || Size < 1)
	{
		return false;
	}

	const uint8 Marker = Data[0];
	int32 NameLength = 0;
	int32 HeaderSize = 0;

	if (Marker >= 0xA0 && Marker <= 0xBF)
	{
		NameLength = Marker & 0x1F;
		HeaderSize = 1;
	}
	else if (Marker == 0xD9)
	{
		if (Size < 2)
		{
			return false;
		}

		NameLength = Data[1];
		HeaderSize = 2;
	}
	else if (Marker == 0xDA)
	{
		if (Size < 3)
		{
			return false;
		}

		NameLength = (Data[1] << 8) | Data[2];
		HeaderSize = 3;
	}
	else if (Marker < 0x80)
	{
		// The SDK's own writers prefix the name with an LEB128 length.
		FCYGBitReader LengthReader(Data, Size);
		NameLength = static_cast<int32>(LengthReader.ReadUInt32());

		if (LengthReader.HasError())
		{
			return false;
		}

		HeaderSize = LengthReader.BytesRead();
	}
	else
	{
		return false;
	}

	if (NameLength <= 0 || NameLength > MaxEventNameBytes || HeaderSize + NameLength > Size)
	{
		return false;
	}

	// Binary payloads share this channel, so a name has to look like a name.
	for (int32 Index = 0; Index < NameLength; ++Index)
	{
		const uint8 Character = Data[HeaderSize + Index];

		if (Character < 0x20 || Character > 0x7E)
		{
			return false;
		}
	}

	OutName = CopyOf(Data + HeaderSize, NameLength);
	OutBodyOffset = HeaderSize + NameLength;
	OutBodySize = Size - OutBodyOffset;
	return true;
}

void UCYGClient::HandleViewDelta(const uint8* Payload, int32 Size)
{
	FCYGBitReader Reader(Payload, Size);
	const int32 ViewId = Reader.ReadInt32();

	if (Reader.HasError())
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("A state delta arrived without a readable view id."));
		return;
	}

	// Views decode their own body, so the packet pointer is handed over untouched.
	OnViewDeltaNative.Broadcast(ViewId, Payload + Reader.BytesRead(), Size - Reader.BytesRead());
}

void UCYGClient::HandleRpc(const uint8* Payload, int32 Size)
{
	FCYGBitReader Reader(Payload, Size);

	// An rpc frame carries its view as four raw big-endian bytes, unlike a delta
	// or a spawn, whose view id is a zig-zag varint.
	const uint32 ViewId = Reader.ReadUInt32BE();
	const uint16 MethodHash = Reader.ReadUInt16BE();
	const uint8 Target = Reader.ReadUInt8();

	if (Reader.HasError())
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("An rpc frame was too short to carry a view, method and target."));
		return;
	}

	OnRpcNative.Broadcast(static_cast<int32>(ViewId), MethodHash, Target, Payload + Reader.BytesRead(), Size - Reader.BytesRead());
}

void UCYGClient::HandleSpawn(const uint8* Payload, int32 Size)
{
	FCYGBitReader Reader(Payload, Size);

	FCYGSpawnRequest Request;
	Request.ViewId = Reader.ReadInt32();
	Request.PrefabId = Reader.ReadUInt16BE();
	Request.OwnerActorNumber = static_cast<int32>(Reader.ReadUInt32());
	Request.Location = FCYGCoordinateSpace::FromWire(Reader.ReadVector3());
	Request.Rotation = FCYGCoordinateSpace::FromWire(Reader.ReadQuaternionRaw());

	if (Reader.HasError())
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("A spawn frame was truncated after %d bytes."), Reader.BytesRead());
		return;
	}

	int32 Remaining = 0;
	Request.Arguments = Reader.Remainder(Remaining);
	Request.ArgumentSize = Remaining;

	OnSpawnNative.Broadcast(Request);
	OnSpawnReceived.Broadcast(Request.ViewId, CopyOfBytes(Request.Arguments, Request.ArgumentSize));
}

void UCYGClient::HandleDespawn(const uint8* Payload, int32 Size)
{
	FCYGBitReader Reader(Payload, Size);
	const int32 ViewId = Reader.ReadInt32();

	if (Reader.HasError())
	{
		return;
	}

	OnDespawnNative.Broadcast(ViewId);
	OnDespawnReceived.Broadcast(ViewId);
}

void UCYGClient::RegisterLocalPlayer()
{
	LocalPlayer = FCYGPlayer();
	LocalPlayer.PlayerId = SeatSessionId;
	LocalPlayer.Nickname = PendingNickname.IsEmpty() ? FPlatformProcess::UserName(false) : PendingNickname;
	LocalPlayer.bIsLocal = true;
	LocalPlayer.bIsMasterClient = true;
	LocalPlayer.ActorNumber = DeriveActorNumber(SeatSessionId);
	LocalPlayer.CustomProperties.Add(TEXT("room"), GetRoomName());

	Players.Reset();
	Players.Add(LocalPlayer.ActorNumber, LocalPlayer);
}

int32 UCYGClient::DeriveActorNumber(const FString& SessionId)
{
	if (SessionId.IsEmpty())
	{
		return 1;
	}

	// The room server keeps its short ids inside the guest module, so each client
	// derives a stable actor number from the session it was handed.
	uint32 Hash = 2166136261u;

	for (int32 Index = 0; Index < SessionId.Len(); ++Index)
	{
		Hash ^= static_cast<uint32>(SessionId[Index]);
		Hash *= 16777619u;
	}

	return static_cast<int32>(Hash & 0x3FFFu) + 1;
}

void UCYGClient::TrackRosterEvent(const FString& EventName, const uint8* Body, int32 Size)
{
	if (EventName.IsEmpty() || LocalPlayer.ActorNumber == 0)
	{
		return;
	}

	if (EventName == EventPlayerJoin)
	{
		FCYGBitReader Reader(Body, Size);
		const int32 Actor = static_cast<int32>(Reader.ReadUInt32());
		const FString Nickname = Reader.Remaining() > 0 ? Reader.ReadString() : FString();

		if (Reader.HasError() || Actor <= 0 || Actor == LocalPlayer.ActorNumber || Players.Contains(Actor))
		{
			return;
		}

		FCYGPlayer Remote;
		Remote.ActorNumber = Actor;
		Remote.PlayerId = FString::Printf(TEXT("actor-%d"), Actor);
		Remote.Nickname = Nickname;
		Players.Add(Actor, Remote);

		ReevaluateMasterClient();
		OnPlayerJoined.Broadcast(Remote);
		return;
	}

	if (EventName == EventPlayerLeave)
	{
		FCYGBitReader Reader(Body, Size);
		const int32 Actor = static_cast<int32>(Reader.ReadUInt32());

		if (Reader.HasError() || !Players.Remove(Actor))
		{
			return;
		}

		ReevaluateMasterClient();
		OnPlayerLeft.Broadcast(Actor);
	}
}

void UCYGClient::ReevaluateMasterClient()
{
	int32 Lowest = TNumericLimits<int32>::Max();

	for (const TPair<int32, FCYGPlayer>& Entry : Players)
	{
		Lowest = FMath::Min(Lowest, Entry.Key);
	}

	for (TPair<int32, FCYGPlayer>& Entry : Players)
	{
		Entry.Value.bIsMasterClient = Lowest != TNumericLimits<int32>::Max() && Entry.Key == Lowest;
	}

	LocalPlayer.bIsMasterClient = Players.Contains(LocalPlayer.ActorNumber)
		&& LocalPlayer.ActorNumber == Lowest;
}

FString UCYGClient::GetRoomName() const
{
	return SeatRoomId.IsEmpty() ? PendingRoomName : SeatRoomId;
}

TArray<FCYGPlayer> UCYGClient::GetPlayers() const
{
	TArray<FCYGPlayer> Result;
	Result.Reserve(Players.Num());

	for (const TPair<int32, FCYGPlayer>& Entry : Players)
	{
		Result.Add(Entry.Value);
	}

	return Result;
}

bool UCYGClient::TryGetPlayer(int32 ActorNumber, FCYGPlayer& OutPlayer) const
{
	const FCYGPlayer* Found = Players.Find(ActorNumber);

	if (Found == nullptr)
	{
		return false;
	}

	OutPlayer = *Found;
	return true;
}
