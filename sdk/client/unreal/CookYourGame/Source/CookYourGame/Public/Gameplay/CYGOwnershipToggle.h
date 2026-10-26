// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Gameplay/CYGBehaviour.h"
#include "CYGOwnershipToggle.generated.h"

class UActorComponent;

/**
 * Turns components on for the player who owns the actor and off for everybody
 * else: the local camera and input here, the remote hitbox there.
 *
 * Nothing is sent. The component never registers with its view, so the dirty mask
 * has no bit for it and a room pays nothing for its existence. It derives from
 * UCYGBehaviour for one reason: the spawn and owner-changed hooks the view already
 * fires, which is how it can react to a transfer of ownership without watching
 * anything of its own.
 */
UCLASS(ClassGroup = (CookYourGame), meta = (BlueprintSpawnableComponent, DisplayName = "CYG Ownership Toggle"))
class COOKYOURGAME_API UCYGOwnershipToggle : public UCYGBehaviour
{
	GENERATED_BODY()

public:
	virtual void OnRegister() override;

	/** Components that run only while the local player owns this actor. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	TArray<TObjectPtr<UActorComponent>> EnableWhenOwner;

	/** Components that run only while somebody else owns it. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	TArray<TObjectPtr<UActorComponent>> DisableWhenOwner;

	/** Re-reads ownership and applies both lists. Safe at any time. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void Apply();

	virtual void OnSpawned() override;

	virtual void OnOwnerChanged(const FCYGPlayer& NewOwner) override;
};
