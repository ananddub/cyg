// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Gameplay/CYGBehaviour.h"
#include "CYGServerToggle.generated.h"

class UActorComponent;

/**
 * Turns components on for whichever peer runs the room and off for the rest: the
 * spawner here, the client-side stand-in there.
 *
 * Like UCYGOwnershipToggle this sends nothing and is never observed by its view. The
 * difference is how it learns its answer. Leading the room is a fact about the
 * roster, and the roster can change without any event that this component is
 * interested in, so the role is polled on the component's own tick behind a change
 * check rather than remembered from a callback that may never come.
 */
UCLASS(ClassGroup = (CookYourGame), meta = (BlueprintSpawnableComponent, DisplayName = "CYG Server Toggle"))
class COOKYOURGAME_API UCYGServerToggle : public UCYGBehaviour
{
	GENERATED_BODY()

public:
	UCYGServerToggle();

	virtual void OnRegister() override;

	virtual void TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction) override;

	/** Components that run only on the peer that leads the room. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	TArray<TObjectPtr<UActorComponent>> EnableWhenAuthority;

	/** Components that run only on the peers that do not. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	TArray<TObjectPtr<UActorComponent>> DisableWhenAuthority;

	/** Count holding this view as authority too, not only leading the room. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	bool bRespectViewAuthority = true;

	/** Re-reads the role and applies both lists. Safe at any time. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void Apply();

	virtual void OnSpawned() override;

	virtual void OnOwnerChanged(const FCYGPlayer& NewOwner) override;

private:
	/** The room's master client, or this peer plus the view when that is allowed. */
	bool HasServerRole() const;

	/** Last role the lists were built for, so a steady room writes nothing. */
	bool bLastRole = false;

	/** False until the first Apply, which has to run whether or not it changed. */
	bool bHasLastRole = false;
};
