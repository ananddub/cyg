// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Components/ActorComponent.h"
#include "Gameplay/CYGSyncVar.h"
#include "Gameplay/CYGObservable.h"
#include "CYGBehaviour.generated.h"

class UCYGView;
struct FCYGPlayer;

/**
 * Base for networked components on a UCYGView: ownership queries, spawn
 * lifecycle and the sync var plumbing.
 *
 * Behaviour blob, written when the component owns registered vars:
 *   [byte dirtyCount] then per dirty var [byte varIndex][value per var codec].
 * A subclass that also syncs its own state overrides SerializeCYG and
 * DeserializeCYG, calls the base first, and lets the reader mirror that order.
 * A subclass whose state has to match a blob the other client writes without a
 * header may skip the base calls, provided it registers no sync vars: see
 * UCYGAttachmentSync.
 */
UCLASS(ClassGroup = (CookYourGame), meta = (BlueprintSpawnableComponent, DisplayName = "CYG Behaviour"))
class COOKYOURGAME_API UCYGBehaviour : public UActorComponent, public ICYGObservable
{
	GENERATED_BODY()

public:
	/** Finds this actor's view, registers nothing yet, and warns if there is no view. */
	virtual void OnRegister() override;

	virtual void OnUnregister() override;

	/** The view this component syncs through, found on the owning actor. */
	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	UCYGView* GetView() const { return View; }

	/** True when the local client may write this component's state. */
	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	bool HasAuthority() const;

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	bool IsMine() const;

	/** Takes ownership of a var, assigns its wire index and starts syncing it. */
	template <typename Type>
	FCYGSyncVar<Type>* RegisterSyncVar(TSharedPtr<FCYGSyncVar<Type>>& Variable);

	/** Registers several vars at once; index order is the order given here. */
	void RegisterSyncVars(TArray<TSharedPtr<FCYGSyncVarBase>>&& Variables);

	/** Adds this component to its view's observed list. */
	void RegisterAsObservable();

	void UnregisterAsObservable();

	/** True when any registered var needs sending. */
	virtual bool HasCYGDelta() const override;

	virtual void SerializeCYG(FCYGBitWriter& Target) override;

	virtual void DeserializeCYG(FCYGBitReader& Source) override;

	/** Calls an RPC on the view's owner: server or host side. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void ServerRpc(const FString& MethodName, const TArray<uint8>& Arguments);

	/** Calls an RPC for every other client in the room. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void ObserversRpc(const FString& MethodName, const TArray<uint8>& Arguments);

	/** Calls an RPC on one player. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void TargetRpc(const FString& MethodName, int32 TargetActorNumber, const TArray<uint8>& Arguments);

	/** C++ override points, fired by the view's lifecycle hooks. */
	virtual void OnSpawned() {}
	virtual void OnDespawned() {}
	virtual void OnOwnerChanged(const FCYGPlayer& NewOwner) {}

	/** Invoked by UCYGView when the actor finished spawning. */
	void InvokeSpawned() { OnSpawned(); }
	void InvokeDespawned() { OnDespawned(); }
	void InvokeOwnerChanged(const FCYGPlayer& NewOwner) { OnOwnerChanged(NewOwner); }

	/** Registered vars, in wire index order; RPC routers and pools may read them. */
	const TArray<TSharedPtr<FCYGSyncVarBase>>& SyncVars() const { return Variables; }

protected:
	UPROPERTY()
	TObjectPtr<UCYGView> View;

private:
	TArray<TSharedPtr<FCYGSyncVarBase>> Variables;
	TSharedPtr<FCYGSyncVarBase> FindVarByIndex(uint8 Index) const;
	bool bObserved = false;
};

template <typename Type>
FCYGSyncVar<Type>* UCYGBehaviour::RegisterSyncVar(TSharedPtr<FCYGSyncVar<Type>>& Variable)
{
	if (!Variable.IsValid())
	{
		return nullptr;
	}

	RegisterSyncVars(TArray<TSharedPtr<FCYGSyncVarBase>>{ Variable });

	return Variable.Get();
}
