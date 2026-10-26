// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGVisibility.h"

#include "Camera/PlayerCameraManager.h"
#include "Components/PrimitiveComponent.h"
#include "Components/SceneComponent.h"
#include "Engine/World.h"
#include "GameFramework/Actor.h"
#include "GameFramework/PlayerController.h"
#include "Gameplay/CYGTransformSync.h"
#include "Gameplay/CYGView.h"

namespace
{
	/** Held behind a function so the map is built when it is first used, not at load. */
	TMap<int32, bool>& GateTable()
	{
		static TMap<int32, bool> Gates;

		return Gates;
	}

	/** Neighbourhood width: the cell an observer is in plus the ring around it. */
	constexpr int32 GridNeighbourhoodCells = 1;
}

bool FCYGVisibilityRule::IsVisibleFrom(const FVector& ObserverLocation, const FVector& TargetLocation, const bool bCurrentlyVisible) const
{
	if (Kind == ECYGVisibilityRuleKind::Grid)
	{
		if (CellSizeCentimetres <= 0.f)
		{
			return true;
		}

		const FIntVector Observer = CellOf(ObserverLocation);
		const FIntVector Target = CellOf(TargetLocation);
		const int32 DeltaX = FMath::Abs(Observer.X - Target.X);
		const int32 DeltaY = FMath::Abs(Observer.Y - Target.Y);

		if (DeltaX > GridNeighbourhoodCells || DeltaY > GridNeighbourhoodCells)
		{
			return false;
		}

		// Height only counts when the rule says it does. In Unreal that axis is Z, where
		// the Unity client measures flat neighbourhoods in Y, so the two clients pick a
		// different letter for the same intent.
		if (!bThreeDimensional)
		{
			return true;
		}

		return FMath::Abs(Observer.Z - Target.Z) <= GridNeighbourhoodCells;
	}

	// The exit radius is the wider one, so an actor sitting on the boundary changes
	// state once per hysteresis band instead of once per frame.
	const float Radius = bCurrentlyVisible
		? VisibilityRadiusCentimetres + CullHysteresisCentimetres
		: VisibilityRadiusCentimetres;

	return FVector::DistSquared(ObserverLocation, TargetLocation) <= static_cast<double>(Radius) * Radius;
}

FIntVector FCYGVisibilityRule::CellOf(const FVector& Location) const
{
	const float Inverse = 1.f / CellSizeCentimetres;

	return FIntVector(
		FMath::FloorToInt(Location.X * Inverse),
		FMath::FloorToInt(Location.Y * Inverse),
		FMath::FloorToInt(Location.Z * Inverse));
}

UCYGVisibility::UCYGVisibility()
{
	PrimaryComponentTick.bCanEverTick = true;
	PrimaryComponentTick.TickGroup = TG_PrePhysics;
}

void UCYGVisibility::OnRegister()
{
	// Before Super, so the tick function is created already knowing it may run.
	PrimaryComponentTick.bCanEverTick = true;

	Super::OnRegister();

	CollectGated();

	// A freshly registered actor is visible until an evaluation says otherwise, which
	// is the safe direction to be wrong in: nothing pops into view a quarter second
	// later, and nothing that should have been drawn is missing while the first
	// evaluation waits.
	bCulled = false;
	AccumulatorSeconds = EvaluationIntervalSeconds;
}

void UCYGVisibility::OnUnregister()
{
	if (GatedViewId > 0)
	{
		FCYGVisibilityGate::Forget(GatedViewId);
		GatedViewId = 0;
	}

	Super::OnUnregister();
}

void UCYGVisibility::TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction)
{
	Super::TickComponent(DeltaTime, TickType, ThisTickFunction);

	AccumulatorSeconds += DeltaTime;

	if (AccumulatorSeconds < EvaluationIntervalSeconds)
	{
		return;
	}

	AccumulatorSeconds = 0.f;
	Evaluate();
}

void UCYGVisibility::CollectGated()
{
	GatedComponents.Reset();
	Smoothing = nullptr;

	const AActor* Owner = GetOwner();

	if (Owner == nullptr)
	{
		return;
	}

	// The actor's own components, which is exactly the set this actor may hide. Primitives
	// a mounted actor owns belong to that actor's visibility component instead.
	TArray<UActorComponent*> Owned;
	Owner->GetComponents(Owned);

	for (UActorComponent* Component : Owned)
	{
		if (UPrimitiveComponent* Primitive = Cast<UPrimitiveComponent>(Component))
		{
			GatedComponents.Add(Primitive);
		}
	}

	// The pose stream is the other thing worth stopping: an actor nobody can see has
	// no reason to spend its interpolation on catching up to where it would have been.
	Smoothing = Owner->FindComponentByClass<UCYGTransformSync>();
}

void UCYGVisibility::Evaluate()
{
	const AActor* Owner = GetOwner();
	const USceneComponent* Root = Owner != nullptr ? Owner->GetRootComponent() : nullptr;

	FVector ObserverLocation;

	// No rules, no owner, or nobody to look from: fail open. The one error worth
	// avoiding here is an empty world because a camera was not where this expected it.
	if (Rules.Num() == 0 || Root == nullptr || !ResolveObserver(ObserverLocation))
	{
		SetCulled(false);
		return;
	}

	SetCulled(!AreRulesSatisfied(Rules, ObserverLocation, Root->GetComponentLocation(), !bCulled));
}

bool UCYGVisibility::ResolveObserver(FVector& OutLocation) const
{
	if (ObserverOverride != nullptr)
	{
		OutLocation = ObserverOverride->GetComponentLocation();
		return true;
	}

	const UWorld* World = GetWorld();
	const APlayerController* Controller = World != nullptr ? World->GetFirstPlayerController() : nullptr;

	if (Controller == nullptr || Controller->PlayerCameraManager == nullptr)
	{
		return false;
	}

	OutLocation = Controller->PlayerCameraManager->GetCameraLocation();
	return true;
}

void UCYGVisibility::SetCulled(const bool bNewCulled)
{
	if (bNewCulled == bCulled)
	{
		return;
	}

	bCulled = bNewCulled;

	for (const TObjectPtr<UPrimitiveComponent>& Primitive : GatedComponents)
	{
		if (UPrimitiveComponent* Target = Primitive.Get())
		{
			Target->SetHiddenInGame(bCulled, true);
		}
	}

	if (Smoothing != nullptr)
	{
		Smoothing->SetComponentTickEnabled(!bCulled);
	}

	const UCYGView* View = GetOwner() != nullptr ? GetOwner()->FindComponentByClass<UCYGView>() : nullptr;
	const int32 ViewId = View != nullptr ? View->ViewId : 0;

	if (ViewId > 0)
	{
		if (GatedViewId > 0 && GatedViewId != ViewId)
		{
			FCYGVisibilityGate::Forget(GatedViewId);
		}

		GatedViewId = ViewId;
		FCYGVisibilityGate::SetGated(ViewId, bCulled);
	}
}

bool UCYGVisibility::AreRulesSatisfied(const TArray<FCYGVisibilityRule>& Rules, const FVector& ObserverLocation, const FVector& TargetLocation, const bool bCurrentlyVisible)
{
	for (const FCYGVisibilityRule& Rule : Rules)
	{
		if (!Rule.IsVisibleFrom(ObserverLocation, TargetLocation, bCurrentlyVisible))
		{
			return false;
		}
	}

	return true;
}

bool FCYGVisibilityGate::ShouldReceive(const int32 ViewId)
{
	const bool* Gated = GateTable().Find(ViewId);

	// Unknown is visible: a view this client has never gated has never been culled.
	return Gated == nullptr || !(*Gated);
}

void FCYGVisibilityGate::SetGated(const int32 ViewId, const bool bGated)
{
	if (ViewId <= 0)
	{
		return;
	}

	GateTable().Add(ViewId, bGated);
}

void FCYGVisibilityGate::Forget(const int32 ViewId)
{
	GateTable().Remove(ViewId);
}

int32 FCYGVisibilityGate::Count()
{
	return GateTable().Num();
}
