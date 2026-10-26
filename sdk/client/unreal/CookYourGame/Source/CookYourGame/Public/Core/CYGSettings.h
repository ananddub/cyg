// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Engine/DeveloperSettings.h"
#include "CYGSettings.generated.h"

/**
 * Endpoints and room defaults a build connects with.
 *
 * Values land in the project's Game config, so a packaged game can point at a
 * different farm without a rebuild:
 *
 *   [/Script/CookYourGame.CYGSettings]
 *   ServerHttpUrl=https://rooms.example.com
 */
UCLASS(Config = Game, DefaultConfig, meta = (DisplayName = "CookYourGame Networking"))
class COOKYOURGAME_API UCYGSettings : public UDeveloperSettings
{
	GENERATED_BODY()

public:
	/** Room used when the game joins without naming one. */
	static const TCHAR* DefaultRoomName;

	/** Matchmake method that attaches to a room or starts it. */
	static const TCHAR* DefaultMatchmakeMethod;

	UCYGSettings();

	/** Base http endpoint of the control plane, for example https://rooms.example.com. */
	UPROPERTY(Config, EditAnywhere, Category = "Endpoints")
	FString ServerHttpUrl;

	/** Path prefix the game socket upgrades on. */
	UPROPERTY(Config, EditAnywhere, Category = "Endpoints")
	FString ServerWsPath;

	/** Room joined when the game does not name one. */
	UPROPERTY(Config, EditAnywhere, Category = "Room")
	FString RoomName;

	/** Capacity advertised to the matchmaker. */
	UPROPERTY(Config, EditAnywhere, Category = "Room", meta = (ClampMin = "1", ClampMax = "255"))
	int32 MaxPlayers;

	/** Wire revision this client speaks. */
	UPROPERTY(Config, EditAnywhere, Category = "Room", meta = (ClampMin = "1"))
	int32 ProtocolVersion;

	/** One of joinOrCreate, join, create, joinById, rejoin, reconnect or spectate. */
	UPROPERTY(Config, EditAnywhere, Category = "Room")
	FString MatchmakeMethod;

	/** Port the room server listens on when the seat reply omits one. */
	UPROPERTY(Config, EditAnywhere, Category = "Endpoints")
	int32 GamePort;

	/** Seat reservation and socket urls stop waiting after this many seconds. */
	UPROPERTY(Config, EditAnywhere, Category = "Timeouts", meta = (ClampMin = "1"))
	int32 RequestTimeoutSeconds;

	/** Milliseconds between heartbeats. Zero stops the heartbeat. */
	UPROPERTY(Config, EditAnywhere, Category = "Timeouts", meta = (ClampMin = "0"))
	int32 PingIntervalMilliseconds;

	/** Counts every packet that passes the link, for the editor profiler. */
	UPROPERTY(Config, EditAnywhere, Category = "Diagnostics")
	bool bProfileTraffic = false;

	/** Heartbeat interval in seconds. */
	float GetPingInterval() const { return FMath::Max(0, PingIntervalMilliseconds) / 1000.f; }

	/** Seat url for a method and room. */
	FString BuildMatchmakeUrl(const FString& Method, const FString& Room) const;

	/** Socket url for a reserved seat; GamePort overrides the reply when it is zero. */
	FString BuildGameSocketUrl(const FString& ProcessId, const FString& Room, const FString& SessionId, const FString& SeatToken, int32 Port) const;

	/** Socket base mirrored from the http endpoint (http becomes ws, https becomes wss). */
	FString BuildSocketBaseUrl() const;

	/** Keeps only letters, digits, '-' and '_', capped at 64 characters. */
	static FString NormalizeRoomName(const FString& Room);

	/** Percent-encodes one path or query segment. */
	static FString EscapeSegment(const FString& Value);

	/** The configured upgrade path with its slashes removed, ready to append. */
	static FString LeadingPath(const FString& Value);

private:
	/** True when the endpoint is usable and writes why it is not. */
	bool TryHttpBase(FString& OutScheme, FString& OutHost, int32& OutPort) const;
};
