// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Gameplay/CYGBehaviour.h"
#include "Gameplay/CYGPoseInterpolator.h"
#include "CYGPhysicsSync.generated.h"

class UPrimitiveComponent;

/**
 * Networked state for a body that is simulating physics.
 *
 * The authority reads the simulated pose and its two velocities on a fixed
 * interval and sends whatever crossed a threshold. A proxy treats those as
 * targets: it interpolates the pose, then writes the pose and both velocities
 * into its own simulation, so the local solver carries the body forward between
 * snapshots instead of being dragged to a standstill each one.
 *
 * This blob is Unreal-specific; the Unity client has no physics sync component,
 * so an actor using it should be spawned on Unreal peers only.
 *
 * Blob after the base sync var block: [byte flags] then, per set flag in order,
 * [packed position], [quaternion], [packed linear velocity], [packed angular
 * velocity in degrees per second].
 */
UCLASS(ClassGroup = (CookYourGame), meta = (BlueprintSpawnableComponent, DisplayName = "CYG Physics Sync"))
class COOKYOURGAME_API UCYGPhysicsSync : public UCYGBehaviour
{
	GENERATED_BODY()

public:
	UCYGPhysicsSync();

	virtual void OnRegister() override;
	virtual void TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction) override;

	/** Which body travels; the owner's first simulating primitive when left unset. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	TObjectPtr<UPrimitiveComponent> Body;

	/** Move and rotation thresholds, in centimetres and degrees. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network", meta = (ClampMin = "0.0"))
	float PositionThresholdCentimetres = 1.0f;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network", meta = (ClampMin = "0.0"))
	float RotationThresholdDegrees = 0.5f;

	/** A proxy snaps instead of interpolating when it is further than this from its target. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network", meta = (ClampMin = "0.0"))
	float CorrectionThresholdCentimetres = 200.0f;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network", meta = (ClampMin = "0.0"))
	float InterpolationDelaySeconds = 0.1f;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network", meta = (ClampMin = "0.0"))
	float MaxExtrapolationSeconds = 0.2f;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network", meta = (ClampMin = "0.001"))
	float SampleIntervalSeconds = 0.02f;

	virtual bool HasCYGDelta() const override;
	virtual void SerializeCYG(FCYGBitWriter& Sink) override;
	virtual void DeserializeCYG(FCYGBitReader& Source) override;

	virtual void OnSpawned() override;

	/** Moves the body and skips the interpolation that would drag it back. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void TeleportBody(const FVector& NewLocation, const FQuat& NewRotation);

private:
	static constexpr uint8 PositionFlag = 1;
	static constexpr uint8 RotationFlag = 2;
	static constexpr uint8 LinearFlag = 4;
	static constexpr uint8 AngularFlag = 8;

	void ResolveBody();
	void SampleAuthority();
	void WritePose(UPrimitiveComponent* Target, const FCYGTrackedPose& Pose) const;

	FCYGPoseInterpolator Smooth;
	double SampleUntilSeconds = 0.0;

	bool bPoseDirty = false;
	uint8 OutFlags = 0;
	FVector OutLocation = FVector::ZeroVector;
	FQuat OutRotation = FQuat::Identity;
	FVector OutLinearVelocity = FVector::ZeroVector;
	FVector OutAngularVelocity = FVector::ZeroVector;

	FVector LastSentLocation = FVector::ZeroVector;
	FQuat LastSentRotation = FQuat::Identity;
	FVector LastSentLinearVelocity = FVector::ZeroVector;
	FVector LastSentAngularVelocity = FVector::ZeroVector;
	bool bHasSentBaseline = false;
};
