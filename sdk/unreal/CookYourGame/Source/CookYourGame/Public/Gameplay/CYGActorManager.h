// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Delegates/Delegate.h"
#include "Gameplay/CYGView.h"
#include "CYGActorManager.generated.h"

class AActor;
class UCYGClient;
struct FCYGSpawnRequest;

/**
 * Turns spawn and despawn frames into actors.
 *
 * A spawn carries a class id, a pose and whatever arguments the owner packed.
 * The actor is created, given its wire identity, and only then does its spawn
 * hook run, so game code sees an actor that is already addressable by id.
 */
UCLASS()
class COOKYOURGAME_API UCYGActorManager : public UObject
{
	GENERATED_BODY()

public:
	/** Binds to a client's lifecycle frames; safe to call again after a reconnect. */
	void AttachTo(UCYGClient* Client, UWorld* World);

	void Detach();

	/** Spawns an actor locally and tells the room about it. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	AActor* SpawnNetworked(TSubclassOf<AActor> ActorClass, const FVector& Location, const FQuat& Rotation, const TArray<uint8>& Arguments);

	/** Local destroy plus the matching room message. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void DestroyNetworked(AActor* Actor);

	/** Applies a spawn frame; called by the client routing, not by game code. */
	AActor* HandleRemoteSpawn(const FCYGSpawnRequest& Request);

	void HandleRemoteDespawn(int32 ViewId);

	/**
	 * Recycles actors of this class rather than destroying them. An actor that
	 * comes back from the pool never sees BeginPlay again, so reset the state in
	 * ICYGPooledActor instead.
	 */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network", meta = (ClampMin = "1"))
	void EnablePooling(TSubclassOf<AActor> ActorClass, int32 Capacity);

	/** Stops recycling a class; whatever is already idle stays until the next Clear. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void DisablePooling(TSubclassOf<AActor> ActorClass);

	/** Creates idle actors now, so the first spawns of a fight cost no construction. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network", meta = (ClampMin = "1"))
	void PrewarmPool(TSubclassOf<AActor> ActorClass, int32 Count);

	/** Idle and handed-out counts, for a profiler row. */
	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	int32 GetPooledActorCount() const;

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	int32 GetActivePooledActorCount() const;

	UPROPERTY()
	TObjectPtr<UCYGClient> Client;

private:
	void OnRemoteSpawn(const FCYGSpawnRequest& Request);
	void OnRemoteDespawn(int32 ViewId);
	void OnRemoteDelta(int32 ViewId, const uint8* Payload, int32 Size);
	void OnRemoteRpc(int32 ViewId, uint16 MethodHash, uint8 Target, const uint8* Arguments, int32 Size);

	/** The view an actor syncs on, created when the class did not ship one. */
	static UCYGView* FindOrCreateView(AActor* Actor);

	/** Takes an actor from the pool when its class is pooled, and spawns one otherwise. */
	AActor* CreateActor(UWorld* Target, UClass* Class, const FTransform& Transform);

	/** Hands an actor back to its pool, or destroys it when the pool is full. */
	void RetireActor(AActor* Actor);

	/** Handles of the client bindings, so Detach can undo exactly what Attach did. */
	FDelegateHandle SpawnHandle;
	FDelegateHandle DespawnHandle;
	FDelegateHandle DeltaHandle;
	FDelegateHandle RpcHandle;

	TWeakObjectPtr<UWorld> World;
};
