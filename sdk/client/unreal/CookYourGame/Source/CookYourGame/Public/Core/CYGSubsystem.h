// Copyright CookYourGame. All rights reserved.

#pragma once

#include "Containers/Ticker.h"
#include "CoreMinimal.h"
#include "Subsystems/GameInstanceSubsystem.h"
#include "CYGSubsystem.generated.h"

class UCYGClient;
class UCYGActorManager;
class UCYGSceneManager;
class UCYGEffects;
class UWorld;

/**
 * Owns the room client for the life of the game engine and pumps it once per
 * frame, so a blueprint never has to remember to drain the socket.
 */
UCLASS()
class COOKYOURGAME_API UCYGSubsystem : public UGameInstanceSubsystem
{
	GENERATED_BODY()

public:
	virtual void Initialize(FSubsystemCollectionBase& Collection) override;
	virtual void Deinitialize() override;

	/** The client this subsystem drives; valid for the life of the game instance. */
	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	UCYGClient* GetClient() const { return Client; }

	/** Creates actors for the spawn frames this client receives. */
	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	UCYGActorManager* GetActorManager() const { return ActorManager; }

	/** Keeps the room on one map; loads the scenes its master client picks. */
	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	UCYGSceneManager* GetSceneManager() const { return SceneManager; }

	/** Sounds and particles that reach the whole room without an actor. */
	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	UCYGEffects* GetEffects() const { return Effects; }

	/** Finds the subsystem from any object in the world, for blueprint use. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network", meta = (WorldContext = "WorldContextObject"))
	static UCYGSubsystem* Get(const UObject* WorldContextObject);

	/** Swaps in a transport before the first connection, for tests and profiling. */
	void UseTransport(const TSharedPtr<class FCYGTransport>& Transport);

private:
	bool PumpInbound(float DeltaSeconds);

	/** Points the actor manager at the level that is live now. */
	UFUNCTION()
	void HandleConnected();

	/** Forgets every view and rpc binding, since their ids died with the room. */
	UFUNCTION()
	void HandleLinkLost(const FString& Reason, int32 CloseCode);

	/** Retires the resume loop, since the seat the loop was chasing is now live. */
	UFUNCTION()
	void HandleRoomTaken(const FString& RoomName);

	/**
	 * Points the actor manager at whatever world is live now, and drops what the
	 * old one held. Watching the world instead of a map-loaded delegate also
	 * catches a travel the game started on its own.
	 */
	void FollowWorldChange();

	UPROPERTY()
	TObjectPtr<UCYGClient> Client;

	UPROPERTY()
	TObjectPtr<UCYGActorManager> ActorManager;

	UPROPERTY()
	TObjectPtr<UCYGSceneManager> SceneManager;

	UPROPERTY()
	TObjectPtr<UCYGEffects> Effects;

	/** The world the manager and the pool are attached to, or null between them. */
	TWeakObjectPtr<UWorld> AttachedWorld;

	/** Named by the engine's own ticker so the member follows the handle type this version returns. */
	decltype(FTSTicker::GetCoreTicker().AddTicker(FTickerDelegate())) TickerHandle{};
};
