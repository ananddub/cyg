// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Components/ActorComponent.h"

/**
 * What "off" means for a component both toggles can agree on.
 *
 * Two calls, because the two kinds of component a designer points these at stop in
 * different ways: a tick-driven simulation has to stop ticking, while a camera or an
 * audio player is off when it is no longer the active one. Neither call is wrong for
 * the other kind, and a component that needs a third meaning should implement
 * OnOwnerChanged itself rather than be listed here.
 */
inline void ApplyCYGComponentGroup(const TArray<TObjectPtr<UActorComponent>>& Group, const bool bEnabled)
{
	for (const TObjectPtr<UActorComponent>& Entry : Group)
	{
		UActorComponent* Target = Entry.Get();

		if (!IsValid(Target))
		{
			continue;
		}

		Target->SetComponentTickEnabled(bEnabled);

		if (bEnabled)
		{
			Target->Activate(true);
		}
		else
		{
			Target->Deactivate();
		}
	}
}
