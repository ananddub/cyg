// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGActorPool.h"

#include "Engine/World.h"
#include "GameFramework/Actor.h"
#include "Gameplay/CYGPoolableActor.h"

namespace
{
	/** Idle actors per class, oldest first. */
	TMap<UClass*, TArray<TWeakObjectPtr<AActor>>> Pools;

	/** Retention cap per class, and the switch that says the class is pooled at all. */
	TMap<UClass*, int32> Capacities;

	/** Actors handed out, so returning one twice cannot put two copies in a queue. */
	TSet<TWeakObjectPtr<AActor>> Actives;

	int32 IdleTotal = 0;
	int32 ActiveTotal = 0;
	int32 CreatedTotal = 0;
}

void FCYGActorPool::EnablePooling(UClass* Class, int32 Capacity)
{
	if (Class == nullptr)
	{
		return;
	}

	Capacities.Add(Class, FMath::Max(1, Capacity));
}

void FCYGActorPool::DisablePooling(UClass* Class)
{
	if (Class != nullptr)
	{
		Capacities.Remove(Class);
	}
}

bool FCYGActorPool::IsPoolingEnabled(UClass* Class)
{
	return Class != nullptr && Capacities.Contains(Class);
}

void FCYGActorPool::Prewarm(UWorld* World, UClass* Class, int32 Count)
{
	if (World == nullptr || Class == nullptr || Count <= 0)
	{
		return;
	}

	const int32* Cap = Capacities.Find(Class);

	if (Cap == nullptr)
	{
		return;
	}

	TArray<TWeakObjectPtr<AActor>>& Queue = Pools.FindOrAdd(Class);

	FActorSpawnParameters Params;
	Params.SpawnCollisionHandlingOverride = ESpawnActorCollisionHandlingMethod::AlwaysSpawn;

	for (int32 Step = 0; Step < Count && Queue.Num() < *Cap; ++Step)
	{
		AActor* Created = World->SpawnActor<AActor>(Class, FTransform::Identity, Params);

		if (Created == nullptr)
		{
			return;
		}

		++CreatedTotal;

		SetInert(Created, true);
		Queue.Add(Created);
		++IdleTotal;
	}
}

AActor* FCYGActorPool::Acquire(UWorld* World, UClass* Class, const FTransform& Transform)
{
	if (World == nullptr || Class == nullptr || !IsPoolingEnabled(Class))
	{
		return nullptr;
	}

	TArray<TWeakObjectPtr<AActor>>& Queue = Pools.FindOrAdd(Class);
	AActor* Reused = nullptr;

	while (Queue.Num() > 0)
	{
		// An idle actor can be torn down from outside, by a level change or a garbage
		// collection; a dead slot is simply not handed out.
		Reused = Queue[0].Get();
		Queue.RemoveAt(0);
		--IdleTotal;

		if (IsValid(Reused))
		{
			break;
		}

		Reused = nullptr;
	}

	if (Reused != nullptr)
	{
		SetInert(Reused, false);
		Reused->SetActorTransform(Transform, false, nullptr, ETeleportType::ResetPhysics);
	}
	else
	{
		FActorSpawnParameters Params;
		Params.SpawnCollisionHandlingOverride = ESpawnActorCollisionHandlingMethod::AlwaysSpawn;

		Reused = World->SpawnActor<AActor>(Class, Transform, Params);

		if (Reused == nullptr)
		{
			return nullptr;
		}

		++CreatedTotal;
	}

	Actives.Add(Reused);
	++ActiveTotal;

	if (Reused->Implements<UCYGPoolableActor>())
	{
		ICYGPooledActor::ExecuteOnCYGSpawnedFromPool(Reused);
	}

	return Reused;
}

bool FCYGActorPool::Release(AActor* Actor)
{
	if (Actor == nullptr || !IsValid(Actor))
	{
		return false;
	}

	UClass* Class = Actor->GetClass();
	const int32* Cap = Capacities.Find(Class);

	if (Cap == nullptr)
	{
		return false;
	}

	if (!Actives.Remove(Actor))
	{
		// Never handed out by the pool, or already back: the caller keeps ownership.
		return false;
	}

	--ActiveTotal;

	if (Actor->Implements<UCYGPoolableActor>())
	{
		ICYGPooledActor::ExecuteOnCYGReturnedToPool(Actor);
	}

	TArray<TWeakObjectPtr<AActor>>& Queue = Pools.FindOrAdd(Class);

	if (Queue.Num() >= *Cap)
	{
		return false;
	}

	SetInert(Actor, true);
	Queue.Add(Actor);
	++IdleTotal;

	return true;
}

void FCYGActorPool::Clear()
{
	for (auto& Pair : Pools)
	{
		for (const TWeakObjectPtr<AActor>& Weak : Pair.Value)
		{
			AActor* Actor = Weak.Get();

			if (Actor == nullptr)
			{
				continue;
			}

			if (UWorld* Target = Actor->GetWorld())
			{
				Target->DestroyActor(Actor);
			}
		}
	}

	Pools.Reset();
	Actives.Reset();
	IdleTotal = 0;
	ActiveTotal = 0;
}

int32 FCYGActorPool::IdleCount()
{
	return IdleTotal;
}

int32 FCYGActorPool::ActiveCount()
{
	return ActiveTotal;
}

int32 FCYGActorPool::TotalCreated()
{
	return CreatedTotal;
}

void FCYGActorPool::SetInert(AActor* Actor, bool bInert)
{
	if (Actor == nullptr)
	{
		return;
	}

	Actor->SetActorHiddenInGame(bInert);
	Actor->SetActorEnableCollision(!bInert);
	Actor->SetActorTickEnabled(!bInert);
}
