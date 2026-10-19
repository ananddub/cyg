// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "UObject/Object.h"
#include "Core/CYGOpCodes.h"
#include "Core/CYGPlayer.h"
#include "CYGClient.generated.h"

class FCYGTransport;
class FCYGWebSocketTransport;
class UCYGSettings;
class IHttpRequest;

/** Room the packet stream is live on and the seat may play. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE(FOnCYGConnected);

/** Raised once the room handshake finished, with the room name. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnCYGJoinedRoom, const FString&, RoomName);

/** Raised when the link drops; CloseCode is the server's close code, 0 when it sent none. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FOnCYGLinkLost, const FString&, Reason, int32, CloseCode);

/** Raised when the server refuses the seat, for example (4002, "room is full"). */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FOnCYGServerError, int32, Code, const FString&, Message);

/** Raised for a named room event; Body is everything after the name. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FOnCYGRoomEvent, const FString&, EventName, const TArray<uint8>&, Body);

/** Raised with authoritative room state, sent on join and after a resume. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnCYGRoomState, const TArray<uint8>&, Payload);

/** Raised for binary the room forwards untouched. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnCYGRoomBytes, const TArray<uint8>&, Payload);

/** Raised after a peer's actor request was decoded; the actor itself is spawned by the actor manager. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FOnCYGSpawnReceived, int32, ViewId, const TArray<uint8>&, Arguments);

/** Raised when a peer destroys a networked actor. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnCYGDespawnReceived, int32, ViewId);

DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnCYGPlayerJoined, const FCYGPlayer&, Player);
DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnCYGPlayerLeft, int32, ActorNumber);

/** Zero-copy view of one incoming delta, for C++ subscribers on the hot path. */
DECLARE_MULTICAST_DELEGATE_ThreeParams(FOnCYGNativeViewDelta, int32 /*ViewId*/, const uint8* /*Payload*/, int32 /*Size*/);
DECLARE_MULTICAST_DELEGATE_FiveParams(FOnCYGNativeRpc, int32 /*ViewId*/, uint16 /*MethodHash*/, uint8 /*Target*/, const uint8* /*Arguments*/, int32 /*Size*/);
DECLARE_MULTICAST_DELEGATE_TwoParams(FOnCYGNativeRoomState, const uint8* /*Payload*/, int32 /*Size*/);
DECLARE_MULTICAST_DELEGATE_TwoParams(FOnCYGNativeRewindHit, const uint8* /*Payload*/, int32 /*Size*/);

/** Everything a peer asked the room to instantiate. */
struct FCYGSpawnRequest
{
	int32 ViewId = 0;
	uint16 PrefabId = 0;
	int32 OwnerActorNumber = 0;
	FVector Location = FVector::ZeroVector;
	FQuat Rotation = FQuat::Identity;
	const uint8* Arguments = nullptr;
	int32 ArgumentSize = 0;
};

DECLARE_MULTICAST_DELEGATE_OneParam(FOnCYGNativeSpawn, const FCYGSpawnRequest& /*Request*/);
DECLARE_MULTICAST_DELEGATE_OneParam(FOnCYGNativeDespawn, int32 /*ViewId*/);

/**
 * The room client: seat reservation, socket link, packet framing and routing.
 *
 * One instance lives per game engine, owned by UCYGSubsystem. Drive it once per
 * frame through the subsystem; heartbeats and queued sends happen there.
 */
UCLASS()
class COOKYOURGAME_API UCYGClient : public UObject
{
	GENERATED_BODY()

public:
	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGConnected OnConnected;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGJoinedRoom OnJoinedRoom;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGLinkLost OnDisconnected;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGServerError OnError;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGRoomEvent OnRoomEvent;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGRoomState OnRoomStateReceived;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGRoomBytes OnRoomBytes;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGSpawnReceived OnSpawnReceived;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGDespawnReceived OnDespawnReceived;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGPlayerJoined OnPlayerJoined;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGPlayerLeft OnPlayerLeft;

	/** C++ hot paths; these fire for every packet and never copy their payload. */
	FOnCYGNativeViewDelta OnViewDeltaNative;
	FOnCYGNativeRpc OnRpcNative;
	FOnCYGNativeRoomState OnRoomStateNative;
	FOnCYGNativeRewindHit OnRewindHitNative;
	FOnCYGNativeSpawn OnSpawnNative;
	FOnCYGNativeDespawn OnDespawnNative;

	/** Reserves a seat over http, then opens the socket and joins. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void ConnectToRoom(const FString& Room, const FString& PlayerName = FString());

	/** Same, asking for one specific matchmake method; empty takes the configured one. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void ConnectToRoomAs(const FString& Room, const FString& Method, const FString& PlayerName = FString());

	/** Joins a match the control plane already placed, using its coordinates directly. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void ConnectToMatch(const FString& MatchId, const FString& Token, const FString& Host = TEXT("127.0.0.1"), int32 Port = 7777, const FString& PlayerName = FString());

	/** Opens a socket on a room url without reserving a seat first. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void Connect(const FString& SocketUrl, const FString& PlayerName = FString());

	/**
	 * Reopens the link on the seat already reserved, so a player who was dropped
	 * comes back with the same actor number instead of joining as somebody new.
	 * False when nothing is known about a seat, or the link is already up.
	 */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	bool ReconnectToSeat();

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void Disconnect();

	/** Frames a payload and hands it to the transport; false when the link is down. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	bool SendPacket(ECYGOpCode Op, const TArray<uint8>& Payload);

	/** One tick of input. The SDK treats the payload as opaque. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	bool SendInputPayload(const TArray<uint8>& Payload);

	/** A delta for one actor: [viewId zigzag][payload]. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	bool SendViewDelta(int32 ViewId, const TArray<uint8>& Payload);

	/** An already encoded call: [viewId u32 big-endian][method hash u16][target u8][arguments]. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	bool SendRpcPayload(const TArray<uint8>& Payload);

	/** A named room event: [name length][utf8 name][body]. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	bool SendRoomEvent(const FString& EventName, const TArray<uint8>& Body);

	/** Tells every peer to instantiate a registered class. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	bool SendSpawnRequest(int32 ViewId, uint16 PrefabId, const FVector& Location, const FQuat& Rotation, const TArray<uint8>& Arguments);

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	bool SendDespawnRequest(int32 ViewId);

	/** Heartbeat and socket pumping; the subsystem calls this every frame. */
	void Tick(float DeltaSeconds);

	/** Reserves the next view id for an owner: (actor << 16) | counter. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	int32 NextViewIdForOwner(int32 ActorNumber);

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network", DisplayName = "IsConnected")
	bool IsLinkConnected() const;

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	bool IsLocalReady() const { return bHandshakeComplete; }

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	int32 GetLocalActorNumber() const { return LocalPlayer.ActorNumber; }

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	bool IsMasterClient() const { return LocalPlayer.bIsMasterClient; }

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	float GetLatencyMilliseconds() const { return LatencyMilliseconds; }

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	FString GetRoomName() const;

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	FString GetSessionId() const { return SeatSessionId; }

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	FString GetReconnectionToken() const { return SeatToken; }

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	int32 GetLastErrorCloseCode() const { return LastErrorCode; }

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	TArray<FCYGPlayer> GetPlayers() const;

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	bool TryGetPlayer(int32 ActorNumber, FCYGPlayer& OutPlayer) const;

	/** Settings from project config, unless a game assigned its own. */
	UPROPERTY(BlueprintReadWrite, Category = "CookYourGame|Network")
	TObjectPtr<UCYGSettings> Settings;

	/** Called before the first connection; transport choice lives here so a test can inject its own. */
	void Initialize();

	/** Close code of the last link, 0 when it dropped without one. */
	int32 GetLastCloseCode() const;

	/** Replaces the transport; only legal while nothing is connected. */
	void UseTransport(const TSharedPtr<FCYGTransport>& InTransport);

	const TSharedPtr<FCYGTransport>& GetTransport() const { return Transport; }

private:
	void EnsureTransport();

	/** Points this client's handlers at whichever transport it currently holds. */
	void BindTransportHandlers();
	void HandleSocketConnected();
	void HandleSocketLost(const FString& Reason, int32 CloseCode);
	void HandleSocketError(const FString& Detail);
	void HandlePacket(const uint8* Packet, int32 Size);
	void Route(ECYGOpCode Op, const uint8* Payload, int32 Size);

	void HandleJoinHandshake(const uint8* Payload, int32 Size);
	void HandleRoomBytes(const uint8* Payload, int32 Size);
	void ForwardRoomEvent(const uint8* Payload, int32 Size);
	void HandleViewDelta(const uint8* Payload, int32 Size);
	void HandleRpc(const uint8* Payload, int32 Size);
	void HandleSpawn(const uint8* Payload, int32 Size);
	void HandleDespawn(const uint8* Payload, int32 Size);

	/** Raises the binary listener with a copy of bytes the caller does not own. */
	void BroadcastRoomBytes(const uint8* Payload, int32 Size);

	void ReportError(int32 Code, const FString& Message);
	void ClearRoomState();
	void RegisterLocalPlayer();
	void ReevaluateMasterClient();
	void TrackRosterEvent(const FString& EventName, const uint8* Body, int32 Size);

	void ReserveSeat(const FString& Room, const FString& PlayerName, const FString& Method = FString());
	void AbandonSeatRequest();
	void OnSeatReply(bool bSucceeded, int32 ResponseCode, const FString& Content);

	/** Tells the resume layer about the seat this client now holds. */
	void CaptureSeatCookie();

	bool SendFrame(ECYGOpCode Op, const uint8* Payload, int32 Size);
	void WriteJoinHandshake(class FCYGBitWriter& Target, const FString& Token) const;

	/** Settings from project config, or the game's override. */
	const UCYGSettings& Configured() const;

	/** Room name a socket url points at: its last path segment. */
	static FString RoomFromUrl(const FString& SocketUrl);

	/** Splits a room message into the event name it starts with and the rest. */
	static bool TrySplitEventName(const uint8* Data, int32 Size, FString& OutName, int32& OutBodyOffset, int32& OutBodySize);

	/** Actor number a session id hashes to; the room server keeps its own ids private. */
	static int32 DeriveActorNumber(const FString& SessionId);

	TSharedPtr<FCYGTransport> Transport;
	TSharedPtr<IHttpRequest, ESPMode::ThreadSafe> SeatRequest;

	FCYGPlayer LocalPlayer;
	TMap<int32, FCYGPlayer> Players;

	FString SeatSessionId;
	FString SeatRoomId;
	FString SeatProcessId;
	FString SeatToken;
	FString PendingNickname;
	FString PendingRoomName;

	int32 SeatGamePort = 0;
	int32 ViewCounter = 0;

	float LatencyMilliseconds = 0.f;
	double LastPingAtSeconds = 0.0;
	double PingSentAtSeconds = 0.0;
	double PingIntervalSeconds = 5.0;

	int32 LastErrorCode = 0;
	FString LastErrorMessage;

	bool bHandshakeComplete = false;
};
