// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGOwnershipToggle.h"

#include "Gameplay/CYGToggleGroups.h"

void UCYGOwnershipToggle::OnRegister()
{
	Super::OnRegister();

	// An actor placed in the level already knows its owner by the time it is
	// registered, so waiting for a spawn hook would leave one frame of the wrong
	// configuration on screen.
	Apply();
}

void UCYGOwnershipToggle::OnSpawned()
{
	Apply();
}

void UCYGOwnershipToggle::OnOwnerChanged(const FCYGPlayer& NewOwner)
{
	(void)NewOwner;

	Apply();
}

void UCYGOwnershipToggle::Apply()
{
	if (GetView() == nullptr)
	{
		// The base class already logged the missing view when this registered.
		return;
	}

	const bool bMine = IsMine();

	ApplyCYGComponentGroup(EnableWhenOwner, bMine);
	ApplyCYGComponentGroup(DisableWhenOwner, !bMine);
}
