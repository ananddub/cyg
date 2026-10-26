// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Components/ActorComponent.h"
#include "Core/CYGPlayer.h"
#include "CYGView.generated.h"

class UCYGClient;
class FCYGBitReader;
class FCYGBitWriter;

/** Raised with the player who spawned or took over a view. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnCYGPlayerEvent, const FCYGPlayer&, Player);

/** Raised just before a view is torn down. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE(FOnCYGViewDespawned);

/** Who may write state: the local actor number and whether it leads the room. */
USTRUCT(BlueprintType)
struct FCYGAuthority
{
	GENERATED_BODY()

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	int32 LocalActorNumber = 0;

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	bool bLocalPlayerIsMaster = false;
};

/**
 * Identity and ownership for a networked actor, and the container for the
 * components whose state rides its deltas.
 *
 * Per-view delta body: [byte dirtyMask] then, for each set bit in ascending
 * component order, that component's blob with no length prefix. The reader
 * walks the same order, which is why component index is wire identity.
 */
UCLASS(ClassGroup = (CookYourGame), meta = (BlueprintSpawnableComponent, DisplayName = "CYG View"))
class COOKYOURGAME_API UCYGView : public UActorComponent
{
	GENERATED_BODY()

public:
	/** Observed components a delta mask can address: one bit each. */
	static constexpr int32 MaxObservedComponents = 8;

	virtual void OnRegister() override;
	virtual void OnUnregister() override;

	/** Wire id of this view, 0 until a spawn or the level assigned one. */
	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame|Network")
	int32 ViewId = 0;

	/** Actor number with authority, 0 for a scene object, -1 while unassigned. */
	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame|Network")
	int32 OwnerActor = -1;

	/** Gives the view its wire identity and puts it in the table. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void AssignViewId(int32 InViewId, int32 InOwnerActor);

	/** Transfers ownership and fires the owner-changed hooks. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void SetOwnerActor(int32 InOwnerActor);

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	bool IsMine() const;

	/** True for an actor placed in the level rather than owned by a player. */
	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	bool IsSceneObject() const { return OwnerActor == 0; }

	/** True when this client may write state for the view. */
	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	bool HasAuthority() const;

	/** Authority test against a snapshot, for the per-frame pump. */
	bool HasAuthority(const FCYGAuthority& Authority) const;

	/** The client this view syncs through, resolved from the owning world. */
	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	UCYGClient* GetClient() const;

	/** Room client for any object in a playing world; null before the game instance exists. */
	static UCYGClient* ResolveClient(const UObject* Context);

	/** Adds an observable, refusing to exceed the dirty mask width. */
	bool AddObservable(UObject* Observable);

	void RemoveObservable(UObject* Observable);

	/** True when at least one observed component reports a delta. */
	bool HasAnyDelta() const;

	/** Writes the dirty mask and every dirty blob. */
	void CollectDeltas(FCYGBitWriter& Target) const;

	/** Reads the dirty mask and hands each blob to its component. */
	void ApplyDeltaStream(FCYGBitReader& Source);

	/** Fires the spawn hooks on the view and its observed behaviours. */
	void InvokeSpawnHooks();

	/** Fires the despawn hooks and drops the view from the table. */
	void InvokeDespawnHooks();

	/** Raised on the local client after a peer spawned this actor. */
	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGPlayerEvent OnViewSpawned;

	/** Raised when ownership moved, with the player who took it over. */
	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGPlayerEvent OnViewOwnerChanged;

	/** Raised just before the actor is torn down. */
	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGViewDespawned OnViewDespawned;

	/** Override point for C++ game code. */
	virtual void OnSpawned() {}

	/** Override point for C++ game code. */
	virtual void OnDespawned() {}

	/** Override point for C++ game code. */
	virtual void OnOwnerChanged(const FCYGPlayer& NewOwner) {}

	/** Components streamed with this view; registration order is wire order. */
	UPROPERTY()
	TArray<TObjectPtr<UObject>> ObservedComponents;

private:
	/** A level object is led by whoever happens to be master client. */
	bool IsSceneAuthority(const FCYGAuthority& Authority) const;
};

/**
 * View id to live view lookup, plus the send pump that turns dirty state into
 * deltas. Game thread only, like the rest of the SDK.
 */
struct COOKYOURGAME_API FCYGViewTable
{
	static void Register(UCYGView* View);
	static void Unregister(int32 ViewId);
	static bool TryGet(int32 ViewId, UCYGView*& OutView);
	static void Clear();

	/** Packs and sends every authoritative view that has something to say. */
	static void Pump(class UCYGClient* Client, const FCYGAuthority& Authority);

	/** Routes one inbound delta body to its view. */
	static void DispatchDelta(int32 ViewId, const uint8* Payload, int32 Size);

	/** Live views, for the actor manager and the profiler. */
	static int32 Count();
};
