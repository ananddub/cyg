// Copyright CookYourGame. All rights reserved.

#pragma once

#include "Containers/Ticker.h"
#include "CoreMinimal.h"
#include "Blueprint/AsyncActionBase.h"
#include "CYGAsyncActions.generated.h"

class UCYGClient;

/** Fires once the room accepted the seat and the handshake finished. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_ThreeParams(FOnCYGJoinSucceeded, FString, RoomName, int32, ActorNumber, FString, SessionId);

/** Fires once for a seat this client could not take: no endpoint, a full room, a dead link. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FOnCYGJoinFailed, int32, Code, FString, Message);

/**
 * A blueprint node that reserves a seat, opens the link and reports the outcome
 * on a pin, so a level blueprint never has to wire three delegates by hand.
 *
 * The node finishes on the first of: the room handshake, a server refusal, a
 * dropped link, or its own deadline. Nothing fires twice.
 */
UCLASS()
class COOKYOURGAME_API UCYGAsyncJoinRoom : public UBlueprintAsyncActionBase
{
	GENERATED_BODY()

public:
	/** Joins with the matchmake method from project settings. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network",
		meta = (BlueprintInternalUseOnly = "true", WorldContext = "WorldContextObject", DisplayName = "Join Room (Async)"))
	static UCYGAsyncJoinRoom* JoinRoom(const UObject* WorldContextObject, const FString& Room,
		const FString& PlayerName = FString(), float TimeoutSeconds = 0.f);

	/** Joins with an explicit method: joinOrCreate, join, create, joinById, rejoin or reconnect. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network",
		meta = (BlueprintInternalUseOnly = "true", WorldContext = "WorldContextObject", DisplayName = "Matchmake (Async)"))
	static UCYGAsyncJoinRoom* Matchmake(const UObject* WorldContextObject, const FString& Room, const FString& Method,
		const FString& PlayerName = FString(), float TimeoutSeconds = 0.f);

	virtual void Activate() override;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGJoinSucceeded OnSuccess;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGJoinFailed OnFailure;

private:
	/** Starts the deadline clock and asks the client for a seat. */
	void Begin(const UObject* WorldContextObject, const FString& Room, const FString& Method,
		const FString& PlayerName, float TimeoutSeconds);

	bool WatchTimeout(float DeltaSeconds);

	/** Stops the watch, drops the client bindings and lets the node be collected. */
	void Finish();

	UFUNCTION()
	void HandleJoinedRoom(const FString& RoomName);

	UFUNCTION()
	void HandleServerError(int32 Code, const FString& Message);

	UFUNCTION()
	void HandleLinkLost(const FString& Reason, int32 CloseCode);

	UPROPERTY()
	TObjectPtr<UObject> Context;

	TWeakObjectPtr<UCYGClient> Client;

	FString Room;
	FString Method;
	FString PlayerName;

	double DeadlineSeconds = 0.0;

	/** Named by the engine's own ticker so the member follows the handle type this version returns. */
	decltype(FTSTicker::GetCoreTicker().AddTicker(FTickerDelegate())) TickerHandle{};

	bool bFinished = false;
};
