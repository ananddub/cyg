// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "CYGSceneManager.generated.h"

class UCYGClient;

/** Raised once the scene a peer was told about is live locally. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnCYGSceneLoaded, const FString&, MapName);

/** Whether the new map replaces the world or is added to it. */
UENUM(BlueprintType)
enum class ECYGSceneLoadMode : uint8
{
	/** Travel to the map: every actor of the old level goes with it. */
	Single = 0,

	/** Stream the map in alongside the current one. */
	Additive = 1
};

/**
 * Keeps the room on one map.
 *
 * Scene changes ride a room event rather than a game opcode, so a server that
 * only relays events still carries them, and the same wire message is read by
 * the Unity client. The frame body is [string map name][byte mode] under the
 * event name CYG_SCENE.
 *
 * Only the master client may start one, because it is the peer whose view of the
 * room the others are following.
 */
UCLASS()
class COOKYOURGAME_API UCYGSceneManager : public UObject
{
	GENERATED_BODY()

public:
	/** Listens to a client's room events; safe to call again after a reconnect. */
	void AttachTo(UCYGClient* InClient);

	void Detach();

	/** Loads the map here and tells every peer to load it too. Master client only. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	bool LoadNetworkedScene(const FString& MapName, ECYGSceneLoadMode Mode);

	/** The map the room last agreed on, empty before the first change. */
	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	const FString& GetCurrentNetworkScene() const { return CurrentNetworkScene; }

	/**
	 * Reports a finished local load; called by the subsystem when the engine
	 * says a map came up, since a level travel outlives this object's call stack.
	 */
	void HandleMapLoaded(UWorld* LoadedWorld);

	/** Raised after a networked scene is live, here or on a peer's instruction. */
	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGSceneLoaded OnNetworkSceneLoaded;

private:
	UFUNCTION()
	void HandleRoomEvent(const FString& EventName, const TArray<uint8>& Body);

	/** Loads without broadcasting a message, which is what a peer's event asks for. */
	void StartLoad(const FString& MapName, ECYGSceneLoadMode Mode);

	UPROPERTY()
	TObjectPtr<UCYGClient> Client;

	FString CurrentNetworkScene;

	/** A travel is still running, so the echo of our own event must be ignored. */
	bool bLoadInFlight = false;
};
