// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGView.h"

#include "CookYourGame.h"
#include "Core/CYGClient.h"
#include "Core/CYGSubsystem.h"
#include "Engine/GameInstance.h"
#include "Engine/World.h"
#include "Gameplay/CYGBehaviour.h"
#include "Gameplay/CYGObservable.h"
#include "Serialization/CYGBitPacker.h"

namespace
{
	TMap<int32, TWeakObjectPtr<UCYGView>> Views;

	/** Observable face of a component, when it implements one. */
	ICYGObservable* AsObservable(UObject* Candidate)
	{
		return Candidate != nullptr ? Cast<ICYGObservable>(Candidate) : nullptr;
	}

	int32 ObservedCount(const UCYGView* View)
	{
		return FMath::Min(View->ObservedComponents.Num(), UCYGView::MaxObservedComponents);
	}
}

void UCYGView::OnRegister()
{
	Super::OnRegister();

	if (ViewId > 0)
	{
		FCYGViewTable::Register(this);
	}
}

void UCYGView::OnUnregister()
{
	if (ViewId > 0)
	{
		FCYGViewTable::Unregister(ViewId);
	}

	Super::OnUnregister();
}

void UCYGView::AssignViewId(int32 InViewId, int32 InOwnerActor)
{
	if (InViewId <= 0)
	{
		UE_LOG(LogCookYourGame, Error, TEXT("AssignViewId needs a positive id; %d was rejected on %s."), InViewId, *GetNameSafe(GetOwner()));
		return;
	}

	if (ViewId > 0 && ViewId != InViewId)
	{
		FCYGViewTable::Unregister(ViewId);
	}

	ViewId = InViewId;
	OwnerActor = InOwnerActor;
	FCYGViewTable::Register(this);
}

void UCYGView::SetOwnerActor(int32 InOwnerActor)
{
	OwnerActor = InOwnerActor;

	FCYGPlayer NewOwner;
	NewOwner.ActorNumber = InOwnerActor;

	UCYGClient* Client = GetClient();

	if (Client != nullptr)
	{
		Client->TryGetPlayer(InOwnerActor, NewOwner);
	}

	for (UObject* Observable : ObservedComponents)
	{
		if (UCYGBehaviour* Behaviour = Cast<UCYGBehaviour>(Observable))
		{
			Behaviour->InvokeOwnerChanged(NewOwner);
		}
	}

	OnOwnerChanged(NewOwner);
	OnViewOwnerChanged.Broadcast(NewOwner);
}

bool UCYGView::IsMine() const
{
	const UCYGClient* Client = GetClient();

	return Client != nullptr && OwnerActor == Client->GetLocalActorNumber();
}

bool UCYGView::IsSceneAuthority(const FCYGAuthority& Authority) const
{
	return IsSceneObject() && Authority.bLocalPlayerIsMaster;
}

bool UCYGView::HasAuthority() const
{
	const UCYGClient* Client = GetClient();

	if (Client == nullptr)
	{
		return false;
	}

	const FCYGAuthority Authority
	{
		Client->GetLocalActorNumber(),
		Client->IsMasterClient()
	};

	return HasAuthority(Authority);
}

bool UCYGView::HasAuthority(const FCYGAuthority& Authority) const
{
	return OwnerActor == Authority.LocalActorNumber || IsSceneAuthority(Authority);
}

bool UCYGView::AddObservable(UObject* Observable)
{
	if (Observable == nullptr || AsObservable(Observable) == nullptr)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("%s does not implement ICYGObservable, so it cannot sync on %s."), *GetNameSafe(Observable), *GetNameSafe(GetOwner()));
		return false;
	}

	if (ObservedComponents.Contains(Observable))
	{
		return false;
	}

	if (ObservedComponents.Num() >= MaxObservedComponents)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("View on %s already observes %d components; the delta mask has no bits left."), *GetNameSafe(GetOwner()), MaxObservedComponents);
		return false;
	}

	ObservedComponents.Add(Observable);
	return true;
}

void UCYGView::RemoveObservable(UObject* Observable)
{
	ObservedComponents.Remove(Observable);
}

bool UCYGView::HasAnyDelta() const
{
	for (int32 Index = 0; Index < ObservedCount(this); ++Index)
	{
		const ICYGObservable* Observable = AsObservable(ObservedComponents[Index]);

		if (Observable != nullptr && Observable->HasCYGDelta())
		{
			return true;
		}
	}

	return false;
}

void UCYGView::CollectDeltas(FCYGBitWriter& Target) const
{
	uint8 Mask = 0;

	for (int32 Index = 0; Index < ObservedCount(this); ++Index)
	{
		const ICYGObservable* Observable = AsObservable(ObservedComponents[Index]);

		if (Observable != nullptr && Observable->HasCYGDelta())
		{
			Mask |= static_cast<uint8>(1u << Index);
		}
	}

	Target.WriteUInt8(Mask);

	for (int32 Index = 0; Index < ObservedCount(this); ++Index)
	{
		if ((Mask & (1u << Index)) == 0)
		{
			continue;
		}

		AsObservable(ObservedComponents[Index])->SerializeCYG(Target);
	}
}

void UCYGView::ApplyDeltaStream(FCYGBitReader& Source)
{
	const uint8 Mask = Source.ReadUInt8();
	const int32 Local = ObservedComponents.Num();

	for (int32 Index = 0; Index < MaxObservedComponents; ++Index)
	{
		if ((Mask & (1u << Index)) == 0)
		{
			continue;
		}

		if (Index >= Local)
		{
			// A component we have not attached yet: version skew, or a prefab that
			// streamed in late. The remaining blobs are undecodable without it.
			UE_LOG(LogCookYourGame, Warning, TEXT("View %d sent component %d but only %d are attached."), ViewId, Index, Local);
			return;
		}

		ICYGObservable* Observable = AsObservable(ObservedComponents[Index]);

		if (Observable != nullptr)
		{
			Observable->DeserializeCYG(Source);
		}
	}
}

void UCYGView::InvokeSpawnHooks()
{
	for (UObject* Observable : ObservedComponents)
	{
		if (UCYGBehaviour* Behaviour = Cast<UCYGBehaviour>(Observable))
		{
			Behaviour->InvokeSpawned();
		}
	}

	OnSpawned();
	OnViewSpawned.Broadcast(FCYGPlayer());
}

void UCYGView::InvokeDespawnHooks()
{
	for (UObject* Observable : ObservedComponents)
	{
		if (UCYGBehaviour* Behaviour = Cast<UCYGBehaviour>(Observable))
		{
			Behaviour->InvokeDespawned();
		}
	}

	OnDespawned();
	OnViewDespawned.Broadcast();
	FCYGViewTable::Unregister(ViewId);
}

UCYGClient* UCYGView::GetClient() const
{
	return ResolveClient(this);
}

UCYGClient* UCYGView::ResolveClient(const UObject* Context)
{
	const UWorld* World = IsValid(Context) ? Context->GetWorld() : nullptr;
	const UGameInstance* Instance = World != nullptr ? World->GetGameInstance() : nullptr;
	const UCYGSubsystem* Subsystem = Instance != nullptr ? Instance->GetSubsystem<UCYGSubsystem>() : nullptr;

	return Subsystem != nullptr ? Subsystem->GetClient() : nullptr;
}

void FCYGViewTable::Register(UCYGView* View)
{
	if (IsValid(View) && View->ViewId > 0)
	{
		Views.Add(View->ViewId, View);
	}
}

void FCYGViewTable::Unregister(int32 ViewId)
{
	if (ViewId > 0)
	{
		Views.Remove(ViewId);
	}
}

bool FCYGViewTable::TryGet(int32 ViewId, UCYGView*& OutView)
{
	const TWeakObjectPtr<UCYGView>* Found = Views.Find(ViewId);

	if (Found == nullptr || !Found->IsValid())
	{
		OutView = nullptr;
		return false;
	}

	OutView = Found->Get();
	return true;
}

void FCYGViewTable::Clear()
{
	Views.Reset();
}

int32 FCYGViewTable::Count()
{
	return Views.Num();
}

void FCYGViewTable::Pump(UCYGClient* Client, const FCYGAuthority& Authority)
{
	if (Client == nullptr || Views.Num() == 0)
	{
		return;
	}

	// Snapshot first: a handler that runs during a send may destroy a view.
	TArray<UCYGView*, TInlineAllocator<64>> Dirty;

	for (const TPair<int32, TWeakObjectPtr<UCYGView>>& Entry : Views)
	{
		UCYGView* View = Entry.Value.Get();

		if (!IsValid(View) || !View->HasAuthority(Authority) || !View->HasAnyDelta())
		{
			continue;
		}

		Dirty.Add(View);
	}

	if (Dirty.Num() == 0)
	{
		return;
	}

	static FCYGBitWriter Body;

	for (UCYGView* View : Dirty)
	{
		Body.Reset();
		View->CollectDeltas(Body);
		Client->SendViewDelta(View->ViewId, TArray<uint8>(Body.Data()));
	}
}

void FCYGViewTable::DispatchDelta(int32 ViewId, const uint8* Payload, int32 Size)
{
	UCYGView* View = nullptr;

	if (ViewId <= 0 || Payload == nullptr || Size <= 0 || !TryGet(ViewId, View))
	{
		return;
	}

	// The payload addresses the receive buffer, so it is decoded before we return.
	FCYGBitReader Source(Payload, Size);
	View->ApplyDeltaStream(Source);
}
