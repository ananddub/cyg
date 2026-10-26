// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGServerToggle.h"

#include "Core/CYGClient.h"
#include "Gameplay/CYGToggleGroups.h"
#include "Gameplay/CYGView.h"

UCYGServerToggle::UCYGServerToggle()
{
	PrimaryComponentTick.bCanEverTick = true;
	PrimaryComponentTick.TickGroup = TG_PrePhysics;
}

void UCYGServerToggle::OnRegister()
{
	// Before Super, so the tick function is created already knowing it may run.
	PrimaryComponentTick.bCanEverTick = true;

	Super::OnRegister();

	Apply();
}

void UCYGServerToggle::TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction)
{
	Super::TickComponent(DeltaTime, TickType, ThisTickFunction);

	// The whole job of this tick is to notice that the answer changed, so a room that
	// has picked its leader and stopped talking costs one bool compare per frame.
	if (!bHasLastRole || HasServerRole() != bLastRole)
	{
		Apply();
	}
}

void UCYGServerToggle::OnSpawned()
{
	Apply();
}

void UCYGServerToggle::OnOwnerChanged(const FCYGPlayer& NewOwner)
{
	(void)NewOwner;

	Apply();
}

bool UCYGServerToggle::HasServerRole() const
{
	const UCYGView* Target = GetView();
	const UCYGClient* Client = Target != nullptr ? Target->GetClient() : nullptr;

	if (Client == nullptr)
	{
		return false;
	}

	if (Client->IsMasterClient())
	{
		return true;
	}

	// A view this peer owns is authoritative for that view alone, which is the case a
	// designer means when they put this component on something a player is driving.
	return bRespectViewAuthority && Target->HasAuthority();
}

void UCYGServerToggle::Apply()
{
	const bool bAuthority = HasServerRole();

	bLastRole = bAuthority;
	bHasLastRole = true;

	ApplyCYGComponentGroup(EnableWhenAuthority, bAuthority);
	ApplyCYGComponentGroup(DisableWhenAuthority, !bAuthority);
}
