// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Gameplay/CYGTransformSync.h"
#include "CYGCharacterMovementSync.generated.h"

class ACharacter;
class UCharacterMovementComponent;

/**
 * Pose sync for an ACharacter, on the same wire blob as UCYGTransformSync.
 *
 * Nothing about the blob changes: a Unity client reading this actor sees an
 * ordinary pose stream. What is Unreal-specific is how the pose is worn. A
 * character with authority runs its own movement component and samples the
 * result. A proxy freezes that component and paints the interpolated pose onto
 * the capsule, because two simulations of the same feet never agree, and a
 * proxy that also simulated would fight every snapshot it was sent.
 *
 * The trade is collision: a frozen capsule can stand inside geometry the
 * authority walked through. Games that need predicted proxy movement should
 * drive the local controller and take authority of the character instead.
 */
UCLASS(ClassGroup = (CookYourGame), meta = (BlueprintSpawnableComponent, DisplayName = "CYG Character Movement Sync"))
class COOKYOURGAME_API UCYGCharacterMovementSync : public UCYGTransformSync
{
	GENERATED_BODY()

public:
	/** Proxies stop simulating and wear the network pose. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	bool bFreezeProxySimulation = true;

	virtual void OnSpawned() override;
	virtual void OnOwnerChanged(const FCYGPlayer& NewOwner) override;

protected:
	virtual void ApplySmoothedPose(const FCYGTrackedPose& Pose) override;
	virtual void OnPoseTeleported(const FVector& Location, const FQuat& Rotation) override;

private:
	/** Hands the movement component the mode this side of authority needs. */
	void ConfigureSimulation();

	UPROPERTY()
	TObjectPtr<ACharacter> Character;

	UPROPERTY()
	TObjectPtr<UCharacterMovementComponent> Movement;
};
