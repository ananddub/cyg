// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGActorManager.h"

#include "CookYourGame.h"
#include "Core/CYGClient.h"
#include "Engine/World.h"
#include "Gameplay/CYGActorPool.h"
#include "Gameplay/CYGClassRegistry.h"
#include "Netcode/CYGRpc.h"

void UCYGActorManager::AttachTo(UCYGClient* InClient, UWorld* InWorld)
{
	Detach();

	Client = InClient;
	World = InWorld;

	if (InClient == nullptr)
	{
		return;
	}

	SpawnHandle = InClient->OnSpawnNative.AddUObject(this, &UCYGActorManager::OnRemoteSpawn);
	DespawnHandle = InClient->OnDespawnNative.AddUObject(this, &UCYGActorManager::OnRemoteDespawn);
	DeltaHandle = InClient->OnViewDeltaNative.AddUObject(this, &UCYGActorManager::OnRemoteDelta);
	RpcHandle = InClient->OnRpcNative.AddUObject(this, &UCYGActorManager::OnRemoteRpc);
}

void UCYGActorManager::Detach()
{
	if (UCYGClient* Previous = Client.Get())
	{
		Previous->OnSpawnNative.Remove(SpawnHandle);
		Previous->OnDespawnNative.Remove(DespawnHandle);
		Previous->OnViewDeltaNative.Remove(DeltaHandle);
		Previous->OnRpcNative.Remove(RpcHandle);
	}

	SpawnHandle.Reset();
	DespawnHandle.Reset();
	DeltaHandle.Reset();
	RpcHandle.Reset();
	Client = nullptr;
}

void UCYGActorManager::OnRemoteSpawn(const FCYGSpawnRequest& Request)
{
	HandleRemoteSpawn(Request);
}

void UCYGActorManager::OnRemoteDespawn(int32 ViewId)
{
	HandleRemoteDespawn(ViewId);
}

void UCYGActorManager::OnRemoteDelta(int32 ViewId, const uint8* Payload, int32 Size)
{
	FCYGViewTable::DispatchDelta(ViewId, Payload, Size);
}

void UCYGActorManager::OnRemoteRpc(int32 ViewId, uint16 MethodHash, uint8 Target, const uint8* Arguments, int32 Size)
{
	FCYGRpcRouter::HandleFrame(ViewId, MethodHash, Target, Arguments, Size);
}

AActor* UCYGActorManager::SpawnNetworked(TSubclassOf<AActor> ActorClass, const FVector& Location, const FQuat& Rotation, const TArray<uint8>& Arguments)
{
	UWorld* Target = World.Get();

	if (Target == nullptr)
	{
		UE_LOG(LogCookYourGame, Error, TEXT("SpawnNetworked has no world; attach the manager to a client first."));
		return nullptr;
	}

	uint16 ClassId = 0;

	if (!UCYGClassRegistry::FindId(ActorClass.Get(), ClassId))
	{
		UE_LOG(LogCookYourGame, Error, TEXT("%s is not in the networked class registry, so it cannot be spawned remotely."), *GetNameSafe(ActorClass.Get()));
		return nullptr;
	}

	const int32 OwnerActor = Client != nullptr ? Client->GetLocalActorNumber() : 0;
	const int32 ViewId = Client != nullptr ? Client->NextViewIdForOwner(OwnerActor) : 0;

	AActor* Spawned = CreateActor(Target, ActorClass.Get(), FTransform(Rotation, Location));

	if (Spawned == nullptr)
	{
		return nullptr;
	}

	UCYGView* View = FindOrCreateView(Spawned);
	View->AssignViewId(ViewId, OwnerActor);

	if (Client != nullptr && ViewId > 0)
	{
		Client->SendSpawnRequest(ViewId, ClassId, Location, Rotation, Arguments);
	}

	View->InvokeSpawnHooks();
	return Spawned;
}

void UCYGActorManager::DestroyNetworked(AActor* Actor)
{
	if (Actor == nullptr)
	{
		return;
	}

	UCYGView* View = Actor->FindComponentByClass<UCYGView>();

	if (View == nullptr)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("DestroyNetworked got %s, which carries no view."), *Actor->GetName());
		return;
	}

	const int32 ViewId = View->ViewId;

	if (Client != nullptr && ViewId > 0)
	{
		Client->SendDespawnRequest(ViewId);
	}

	View->InvokeDespawnHooks();
	RetireActor(Actor);
}

AActor* UCYGActorManager::HandleRemoteSpawn(const FCYGSpawnRequest& Request)
{
	UWorld* Target = World.Get();
	UClass* Class = UCYGClassRegistry::FindClass(Request.PrefabId);

	if (Target == nullptr || Class == nullptr)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("A peer asked for class id %d, which this build does not know."), Request.PrefabId);
		return nullptr;
	}

	UCYGView* Known = nullptr;

	if (FCYGViewTable::TryGet(Request.ViewId, Known) && Known != nullptr)
	{
		// A late joiner or a resent spawn: reuse the actor rather than doubling it.
		return Known->GetOwner();
	}

	AActor* Spawned = CreateActor(Target, Class, FTransform(Request.Rotation, Request.Location));

	if (Spawned == nullptr)
	{
		return nullptr;
	}

	UCYGView* Existing = FindOrCreateView(Spawned);
	Existing->AssignViewId(Request.ViewId, Request.OwnerActorNumber);
	Existing->InvokeSpawnHooks();

	return Spawned;
}

void UCYGActorManager::HandleRemoteDespawn(int32 ViewId)
{
	UCYGView* View = nullptr;

	if (!FCYGViewTable::TryGet(ViewId, View) || View == nullptr)
	{
		return;
	}

	AActor* Owner = View->GetOwner();

	View->InvokeDespawnHooks();
	RetireActor(Owner);
}

AActor* UCYGActorManager::CreateActor(UWorld* Target, UClass* Class, const FTransform& Transform)
{
	if (AActor* Pooled = FCYGActorPool::Acquire(Target, Class, Transform))
	{
		return Pooled;
	}

	FActorSpawnParameters Params;
	Params.SpawnCollisionHandlingOverride = ESpawnActorCollisionHandlingMethod::AlwaysSpawn;

	return Target->SpawnActor<AActor>(Class, Transform, Params);
}

void UCYGActorManager::RetireActor(AActor* Actor)
{
	if (Actor == nullptr)
	{
		return;
	}

	if (FCYGActorPool::Release(Actor))
	{
		return;
	}

	if (UWorld* Target = Actor->GetWorld())
	{
		Target->DestroyActor(Actor);
	}
}

void UCYGActorManager::EnablePooling(TSubclassOf<AActor> ActorClass, int32 Capacity)
{
	FCYGActorPool::EnablePooling(ActorClass.Get(), Capacity > 0 ? Capacity : FCYGActorPool::DefaultCapacity);
}

void UCYGActorManager::DisablePooling(TSubclassOf<AActor> ActorClass)
{
	FCYGActorPool::DisablePooling(ActorClass.Get());
}

void UCYGActorManager::PrewarmPool(TSubclassOf<AActor> ActorClass, int32 Count)
{
	FCYGActorPool::Prewarm(World.Get(), ActorClass.Get(), Count);
}

int32 UCYGActorManager::GetPooledActorCount() const
{
	return FCYGActorPool::IdleCount();
}

int32 UCYGActorManager::GetActivePooledActorCount() const
{
	return FCYGActorPool::ActiveCount();
}

UCYGView* UCYGActorManager::FindOrCreateView(AActor* Actor)
{
	if (UCYGView* Existing = Actor->FindComponentByClass<UCYGView>())
	{
		return Existing;
	}

	UCYGView* Created = NewObject<UCYGView>(Actor);
	Created->RegisterComponent();
	Actor->AddInstanceComponent(Created);

	return Created;
}
