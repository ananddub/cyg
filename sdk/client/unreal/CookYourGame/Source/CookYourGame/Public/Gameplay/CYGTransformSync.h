// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Gameplay/CYGBehaviour.h"
#include "Gameplay/CYGPoseInterpolator.h"
#include "CYGTransformSync.generated.h"

class USceneComponent;

/**
 * Networked world pose, on the wire in the same shape the Unity client reads.
 *
 * The authority samples its pose on a fixed interval and sends only what moved
 * past a threshold; a proxy buffers the samples and draws them a tenth of a
 * second late, so jitter arrives off-screen. A jump of five metres or more is
 * read as a teleport, and both sides clear the buffer instead of sliding across
 * the map. An authoritative client decodes the pose it is told but never writes
 * it: its own simulation owns the transform.
 *
 * Blob after the base sync var block: [byte flags] then, for each set flag in
 * order, [packed position], [quaternion], [scale as three singles].
 */
UCLASS(ClassGroup = (CookYourGame), meta = (BlueprintSpawnableComponent, DisplayName = "CYG Transform Sync"))
class COOKYOURGAME_API UCYGTransformSync : public UCYGBehaviour
{
	GENERATED_BODY()

public:
	UCYGTransformSync();

	virtual void OnRegister() override;
	virtual void TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction) override;

	/** Also sync relative scale, which is authored rather than simulated in most games. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	bool bSyncScale = false;

	/** How far behind the clock a proxy draws; below the round trip this starves. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network", meta = (ClampMin = "0.0"))
	float InterpolationDelaySeconds = 0.1f;

	/** Longest a proxy coasts on the last known velocity when snapshots stop. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network", meta = (ClampMin = "0.0"))
	float MaxExtrapolationSeconds = 0.2f;

	/** Authority sampling period, matching the Unity client's fixed network tick. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network", meta = (ClampMin = "0.001"))
	float SampleIntervalSeconds = 0.02f;

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	USceneComponent* GetTargetComponent() const { return Target; }

	/** Moves the pose and tells the room it was a teleport, not a move. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void TeleportTo(const FVector& NewLocation);

	/** Turns the pose and tells the room it was a teleport, not a turn. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void TeleportRotation(const FQuat& NewRotation);

	virtual bool HasCYGDelta() const override;
	virtual void SerializeCYG(FCYGBitWriter& Sink) override;
	virtual void DeserializeCYG(FCYGBitReader& Source) override;

	virtual void OnSpawned() override;

	/** Wire flags for the pose fields this delta carries; see the class comment. */
	static constexpr uint8 PositionFlag = 1;
	static constexpr uint8 RotationFlag = 2;
	static constexpr uint8 ScaleFlag = 4;
	static constexpr uint8 TeleportFlag = 8;

protected:
	/** Whose pose travels; the owning actor's root unless this says otherwise. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	TObjectPtr<USceneComponent> Target;

	/** Called with the pose a proxy should wear this frame. */
	virtual void ApplySmoothedPose(const FCYGTrackedPose& Pose);

	/** Called when a teleport clears the buffer, so a subclass can follow. */
	virtual void OnPoseTeleported(const FVector& Location, const FQuat& Rotation) { (void)Location; (void)Rotation; }

	/** Adds these wire flags to what the next delta carries. */
	void MarkPoseDirty(uint8 Flags);

private:
	void ResolveTarget();
	void SampleAuthority();

	FCYGPoseInterpolator Smooth;
	double SampleUntilSeconds = 0.0;

	bool bPoseDirty = false;
	uint8 OutFlags = 0;
	FVector OutLocation = FVector::ZeroVector;
	FQuat OutRotation = FQuat::Identity;
	FVector OutScale = FVector::OneVector;

	FVector LastSentLocation = FVector::ZeroVector;
	FQuat LastSentRotation = FQuat::Identity;
	FVector LastSentScale = FVector::OneVector;
	bool bHasSentBaseline = false;

	FVector LastIncomingLocation = FVector::ZeroVector;
	FQuat LastIncomingRotation = FQuat::Identity;
	double LastIncomingSeconds = 0.0;
	bool bHasIncomingBaseline = false;
};
