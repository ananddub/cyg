// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGCharacterMovementSync.h"

#include "CookYourGame.h"
#include "GameFramework/Character.h"
#include "GameFramework/CharacterMovementComponent.h"

void UCYGCharacterMovementSync::OnSpawned()
{
	UCYGTransformSync::OnSpawned();

	AActor* Owner = GetOwner();
	Character = Cast<ACharacter>(Owner);
	Movement = Character != nullptr ? Character->GetCharacterMovement() : nullptr;

	if (Character == nullptr)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("%s needs an ACharacter owner to sync movement."), *GetName());
	}

	ConfigureSimulation();
}

void UCYGCharacterMovementSync::OnOwnerChanged(const FCYGPlayer& NewOwner)
{
	(void)NewOwner;

	// The snapshot history is stale the moment authority moves, so start the
	// buffer over from wherever the character stands right now.
	OnSpawned();
}

void UCYGCharacterMovementSync::ConfigureSimulation()
{
	if (Movement == nullptr)
	{
		return;
	}

	// Stopping the tick is the whole freeze: gravity, stepping and mode changes
	// all live in it, and the movement mode itself is left alone so that taking
	// authority back does not have to guess which mode the character was in.
	Movement->SetComponentTickEnabled(HasAuthority() || !bFreezeProxySimulation);
}

void UCYGCharacterMovementSync::ApplySmoothedPose(const FCYGTrackedPose& Pose)
{
	UCYGTransformSync::ApplySmoothedPose(Pose);

	if (Movement != nullptr)
	{
		// A frozen component keeps a velocity only so that taking authority later
		// does not start from whatever the last local frame left behind.
		Movement->Velocity = Pose.Velocity;
	}
}

void UCYGCharacterMovementSync::OnPoseTeleported(const FVector& Location, const FQuat& Rotation)
{
	(void)Location;
	(void)Rotation;

	if (Movement != nullptr)
	{
		Movement->Velocity = FVector::ZeroVector;
	}
}
