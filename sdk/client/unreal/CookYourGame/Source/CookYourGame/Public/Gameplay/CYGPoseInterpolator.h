// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"

/** One sampled world pose, in Unreal units. */
struct FCYGTrackedPose
{
	double TimeSeconds = 0.0;
	FVector Location = FVector::ZeroVector;
	FQuat Rotation = FQuat::Identity;
	FVector Velocity = FVector::ZeroVector;

	/** Only a body with a simulation needs this; a plain pose leaves it zero. */
	FVector AngularVelocity = FVector::ZeroVector;
};

/** Shared maths for pose smoothing, kept out of the components that use it. */
namespace CYGPoseMath
{
	/** Smallest angle between two rotations, in radians. */
	float AngleBetween(const FQuat& First, const FQuat& Second);
}

/**
 * Snapshot ring and the smoothing every pose component shares.
 *
 * A proxy draws the past: the caller subtracts its interpolation delay from the
 * clock and asks for the pose at that time. Between two snapshots the location
 * follows a cubic Hermite curve whose tangents are each end's velocity, which
 * keeps the path C1 continuous across a join, so a proxy does not kink every
 * time a new snapshot lands. Rotation takes the short arc between the same two
 * samples. Past the newest snapshot the last velocity carries the pose, for no
 * longer than the caller's extrapolation cap.
 */
class COOKYOURGAME_API FCYGPoseInterpolator
{
public:
	/** Snapshots held; at fifty per second this is a little over half a second. */
	static constexpr int32 Capacity = 32;

	void Reset();

	/** Adds the newest sample, overwriting the oldest once the ring is full. */
	void Push(const FCYGTrackedPose& Pose);

	/** Drops the history and starts again from one pose: a teleport or a first frame. */
	void Restart(const FCYGTrackedPose& Pose);

	int32 Count() const { return Num; }
	bool IsEmpty() const { return Num == 0; }

	const FCYGTrackedPose& Newest() const { return At(Num - 1); }
	const FCYGTrackedPose& Oldest() const { return At(0); }

	/** Pose as of RequestedSeconds, which the caller has already shifted into the past. */
	FCYGTrackedPose Sample(double RequestedSeconds, double MaxExtrapolationSeconds) const;

private:
	const FCYGTrackedPose& At(int32 IndexFromOldest) const;

	FCYGTrackedPose Poses[Capacity];
	int32 Head = 0;
	int32 Num = 0;
};
