// Copyright CookYourGame. All rights reserved.

#include "CYGActiveLink.h"

#include "Core/CYGClient.h"
#include "Core/CYGSubsystem.h"
#include "UObject/UObjectIterator.h"

UCYGClient* FCYGActiveLink::Find()
{
	UCYGClient* Fallback = nullptr;

	for (TObjectIterator<UCYGSubsystem> Iterator; Iterator; ++Iterator)
	{
		const UCYGSubsystem* Subsystem = *Iterator;

		if (!IsValid(Subsystem))
		{
			continue;
		}

		UCYGClient* Candidate = Subsystem->GetClient();

		if (Candidate == nullptr)
		{
			continue;
		}

		if (Candidate->IsLinkConnected())
		{
			return Candidate;
		}

		if (Fallback == nullptr)
		{
			Fallback = Candidate;
		}
	}

	// A client that never connected is still worth reading: it carries the room
	// name it was told to join and the close code that stopped it getting there.
	return Fallback;
}
