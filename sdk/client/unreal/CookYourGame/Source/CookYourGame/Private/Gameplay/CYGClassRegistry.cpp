// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGClassRegistry.h"

#include "CookYourGame.h"

namespace
{
	TMap<uint16, TWeakObjectPtr<UClass>> ClassesById;
	TMap<UClass*, uint16> IdsByClass;
}

void UCYGClassRegistry::PostLoad()
{
	Super::PostLoad();

	for (const FCYGNetworkedClass& Entry : NetworkedClasses)
	{
		if (Entry.ActorClass != nullptr)
		{
			Register(Entry.ActorClass, static_cast<uint16>(FMath::Clamp(Entry.NetworkId, 0, 65535)));
		}
	}
}

void UCYGClassRegistry::Activate()
{
	Clear();
	PostLoad();
}

void UCYGClassRegistry::Register(TSubclassOf<AActor> ActorClass, uint16 NetworkId)
{
	UClass* Resolved = ActorClass.Get();

	if (Resolved == nullptr)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("Class id %d names no class."), NetworkId);
		return;
	}

	if (const uint16* Existing = IdsByClass.Find(Resolved))
	{
		ClassesById.Remove(*Existing);
	}

	const uint16* Taken = ClassesById.Find(NetworkId);

	if (Taken != nullptr)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("Id %d was held by %s and now goes to %s."), NetworkId, *GetNameSafe(Taken->Get()), *Resolved->GetName());
		IdsByClass.Remove(Taken->Get());
	}

	ClassesById.Add(NetworkId, Resolved);
	IdsByClass.Add(Resolved, NetworkId);
}

UClass* UCYGClassRegistry::FindClass(uint16 NetworkId)
{
	const TWeakObjectPtr<UClass>* Found = ClassesById.Find(NetworkId);

	return Found != nullptr ? Found->Get() : nullptr;
}

bool UCYGClassRegistry::FindId(const UClass* ActorClass, uint16& OutNetworkId)
{
	if (ActorClass == nullptr)
	{
		return false;
	}

	// A subclass inherits the id of the closest registered ancestor, so a game
	// can register one base class and spawn variants of it.
	for (const TPair<UClass*, uint16>& Entry : IdsByClass)
	{
		if (ActorClass == Entry.Key || ActorClass->IsChildOf(Entry.Key))
		{
			OutNetworkId = Entry.Value;
			return true;
		}
	}

	return false;
}

void UCYGClassRegistry::Clear()
{
	ClassesById.Reset();
	IdsByClass.Reset();
}

int32 UCYGClassRegistry::Count()
{
	return ClassesById.Num();
}
