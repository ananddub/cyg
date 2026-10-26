// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Components/ActorComponent.h"
#include "CYGLagCompensation.generated.h"

class AActor;
class UPrimitiveComponent;
class UCYGClient;

/** One recorded collider pose, in world space and Unreal units. */
struct FCYGRollbackSnapshot
{
	double TimeSeconds = 0.0;
	FVector Center = FVector::ZeroVector;

	/** Half extents, so a sphere records its radius in X and leaves the rest zero. */
	FVector Extents = FVector::ZeroVector;

	bool bIsSphere = false;
};

/** A hit that was reconstructed from history rather than from a live collider. */
USTRUCT(BlueprintType)
struct FCYGRewindHit
{
	GENERATED_BODY()

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	FVector Location = FVector::ZeroVector;

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	FVector Normal = FVector::ZeroVector;

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	double Distance = 0.0;

	/** The clock the pose was rebuilt from, which the shooter named if it was in range. */
	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	double TimeSeconds = 0.0;

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	TWeakObjectPtr<AActor> Actor;
};

/** What a shooter reports having fired, before anyone agrees on whether it hit. */
struct FCYGHitClaim
{
	double TimeSeconds = 0.0;
	FVector Origin = FVector::ZeroVector;
	FVector Direction = FVector::ZeroVector;
	int32 TargetViewId = 0;
};

/**
 * Keeps one collider's recent world poses so a shot can be tested against the past.
 *
 * Attach it next to any collision component. Every sixtieth of a second it stores that
 * body's bounds, and the rewind index looks the stored pose up for the moment a claim
 * names. Recording stops at the capacity, which is why the window is short: history is
 * what costs memory here, not the maths.
 */
UCLASS(ClassGroup = (CookYourGame), meta = (BlueprintSpawnableComponent, DisplayName = "CYG Rollback Buffer"))
class COOKYOURGAME_API UCYGRollbackBuffer : public UActorComponent
{
	GENERATED_BODY()

public:
	UCYGRollbackBuffer();

	virtual void OnRegister() override;
	virtual void OnUnregister() override;
	virtual void TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction) override;

	/** Samples held; 120 is two seconds at the capture rate. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network", meta = (ClampMin = "2"))
	int32 Capacity = 120;

	/** Body to record; the owner's first primitive when left unset. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	TObjectPtr<UPrimitiveComponent> Body;

	/** Test a round body analytically instead of by its enclosing box. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	bool bTreatAsSphere = false;

	/** Pose at Seconds, built from the two samples around it. False when nothing reaches back that far. */
	bool TrySampleAt(double Seconds, FCYGRollbackSnapshot& OutSample) const;

	/** The channel this body belongs to, which a rewind query filters on. */
	uint8 CollisionChannel() const;

private:
	void ResolveBody();
	void CaptureNow();

	TArray<FCYGRollbackSnapshot> Samples;
	int32 Head = 0;
	int32 Num = 0;
	double LastCaptureSeconds = 0.0;
};

/**
 * Lag compensation: the rewind index, and the claim both sides of a shot agree on.
 *
 * A shooter tests its own shot, then sends the ray and the moment it was fired. The
 * authority rebuilds every recorded collider at that moment and tests the same ray
 * again; if it agrees, the authority applies the hit it computed rather than the one
 * the shooter reported, so a claim can never invent a hit.
 *
 * The claim's timestamp is in the sender's own clock. Only differences are compared,
 * so the two sides have to run clocks that advance at the same rate, and
 * NowOffsetSeconds is where a game that has measured the gap puts it.
 */
struct COOKYOURGAME_API FCYGLagCompensation
{
	/** Rewinds older than this are refused: beyond it, history is guesswork. */
	static constexpr double MaxRewindWindowSeconds = 0.3;

	/** Added to the local clock to stand in for the authority's. */
	static double NowOffsetSeconds;

	static void RegisterBuffer(UPrimitiveComponent* Body, UCYGRollbackBuffer* Buffer);
	static void UnregisterBuffer(UPrimitiveComponent* Body);

	/** Drops the index; the subsystem calls this when a room link goes away. */
	static void ClearBuffers();

	/** Bodies recorded, for the profiler. */
	static int32 BufferCount();

	/**
	 * Tests a ray against colliders as they were at ClientSeconds.
	 *
	 * ChannelMask carries one bit per collision channel and lets everything through
	 * when zero; IgnoreActor is left out of the result, so a shooter cannot hit itself.
	 */
	static bool RaycastRewind(const FVector& Origin, const FVector& Direction, double MaxDistance,
		double ClientSeconds, uint64 ChannelMask, const AActor* IgnoreActor, FCYGRewindHit& OutHit);

	/** Sends [time as single][origin metres][direction][target view id zigzag]. */
	static bool SubmitHitClaim(UCYGClient* Client, double ClientSeconds, const FVector& ShotOrigin,
		const FVector& ShotDirection, int32 TargetViewId);

	/** Decodes a claim a peer sent. Axis-permuted back into Unreal space. */
	static bool TryReadClaim(const uint8* Payload, int32 Size, FCYGHitClaim& OutClaim);
};
