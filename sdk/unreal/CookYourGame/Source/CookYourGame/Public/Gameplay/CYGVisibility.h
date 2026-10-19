// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Components/ActorComponent.h"
#include "CYGVisibility.generated.h"

class UPrimitiveComponent;
class UCYGTransformSync;
class USceneComponent;

/** Which test a visibility rule runs. */
UENUM(BlueprintType)
enum class ECYGVisibilityRuleKind : uint8
{
	/** Hidden once the observer is further away than the radius. */
	Distance = 0 UMETA(DisplayName = "Distance"),

	/** Hidden once the observer is outside the cell and its neighbours. */
	Grid = 1 UMETA(DisplayName = "Grid"),
};

/**
 * One culling test, authored as data so a rule set is a list rather than a class.
 *
 * Distances are centimetres, the engine's own unit: the Unity client's 50 metres is
 * this rule's 5000.
 */
USTRUCT(BlueprintType)
struct FCYGVisibilityRule
{
	GENERATED_BODY()

public:
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame")
	ECYGVisibilityRuleKind Kind = ECYGVisibilityRuleKind::Distance;

	/** Distance at which a hidden target becomes visible again. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame", meta = (ClampMin = "1"))
	float VisibilityRadiusCentimetres = 5000.f;

	/** Extra distance an already visible target may travel before it is culled again. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame", meta = (ClampMin = "0"))
	float CullHysteresisCentimetres = 500.f;

	/** World size of one grid cell. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame", meta = (ClampMin = "1"))
	float CellSizeCentimetres = 5000.f;

	/** True compares all three axes; false compares the ground plane and ignores height. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame")
	bool bThreeDimensional = true;

	/**
	 * True when a target at the given place may be drawn for an observer at another.
	 * The current visibility is an input, not a cached answer: the radius rule needs it
	 * to know which side of the hysteresis band to measure from.
	 */
	bool IsVisibleFrom(const FVector& ObserverLocation, const FVector& TargetLocation, bool bCurrentlyVisible) const;

private:
	/** Cell coordinates of a location, which is how the grid rule compares two places. */
	FIntVector CellOf(const FVector& Location) const;
};

/**
 * Stops drawing a networked actor that the local player cannot see.
 *
 * This is a presentation gate, not an interest filter: the room still sends, and the
 * bytes still arrive. That is what makes it safe to run without the server agreeing,
 * and it is why the state is published through FCYGVisibilityGate rather than used to
 * drop anything: a game that wants to stop *processing* what it cannot see can read
 * the gate where it handles deltas.
 *
 * Rules are ANDed, and no rules at all means everything stays visible. Evaluation is
 * on an interval rather than every frame, because the test is a distance and the
 * answer almost never changes within one.
 */
UCLASS(ClassGroup = (CookYourGame), meta = (BlueprintSpawnableComponent, DisplayName = "CYG Visibility"))
class COOKYOURGAME_API UCYGVisibility : public UActorComponent
{
	GENERATED_BODY()

public:
	UCYGVisibility();

	virtual void OnRegister() override;

	virtual void OnUnregister() override;

	virtual void TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction) override;

	/** Every rule that has to pass; an empty list hides nothing. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	TArray<FCYGVisibilityRule> Rules;

	/** Where the observer is, when the local player's camera is the wrong origin. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	TObjectPtr<USceneComponent> ObserverOverride;

	/** Seconds between two evaluations of this actor. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network", meta = (ClampMin = "0.01"))
	float EvaluationIntervalSeconds = 0.25f;

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame|Network")
	bool bCulled = false;

	/** True when every rule lets this pair of places see each other. */
	static bool AreRulesSatisfied(const TArray<FCYGVisibilityRule>& Rules, const FVector& ObserverLocation, const FVector& TargetLocation, bool bCurrentlyVisible);

protected:
	/** Primitives switched off while culled: the primitives this actor owns. */
	UPROPERTY()
	TArray<TObjectPtr<UPrimitiveComponent>> GatedComponents;

	/** The pose component frozen while culled, so a hidden actor stops chasing snapshots. */
	UPROPERTY()
	TObjectPtr<UCYGTransformSync> Smoothing;

private:
	void Evaluate();

	/** Camera position to measure from, false when there is nobody to measure for. */
	bool ResolveObserver(FVector& OutLocation) const;

	void SetCulled(bool bNewCulled);

	void CollectGated();

	float AccumulatorSeconds = 0.f;

	/** The view id this component last gated, so unregistering can clear the right entry. */
	int32 GatedViewId = 0;
};

/**
 * Which views the local client has stopped drawing.
 *
 * Advisory: nothing in the connection path reads it. It exists so a game that wants
 * to skip work for culled actors (expensive delta handlers, animation, audio)
 * has one question to ask, and so the state a visibility component already computed
 * is not locked inside the component that computed it.
 */
struct COOKYOURGAME_API FCYGVisibilityGate
{
	/** False only for a view a UCYGVisibility component is currently hiding. */
	static bool ShouldReceive(int32 ViewId);

	/** Records the gate for a view. Ids that are not assigned yet are ignored. */
	static void SetGated(int32 ViewId, bool bGated);

	/** Drops a view's entry, for when its component goes away. */
	static void Forget(int32 ViewId);

	/** Entries currently held, for a debug screen. */
	static int32 Count();
};
