// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGPoseInterpolator.h"

float CYGPoseMath::AngleBetween(const FQuat& First, const FQuat& Second)
{
	// Quaternions describe the same turn at both poles, so the magnitude of the dot
	// product is what carries the angle, and only the closer pole needs to be considered.
	const double CosHalfAngle = FMath::Clamp(FMath::Abs(FQuat::FastDot(First, Second)), 0.0, 1.0);

	return static_cast<float>(2.0 * FMath::Acos(CosHalfAngle));
}

void FCYGPoseInterpolator::Reset()
{
	Head = 0;
	Num = 0;
}

void FCYGPoseInterpolator::Push(const FCYGTrackedPose& Pose)
{
	Poses[Head] = Pose;
	Head = (Head + 1) % Capacity;

	if (Num < Capacity)
	{
		++Num;
	}
}

void FCYGPoseInterpolator::Restart(const FCYGTrackedPose& Pose)
{
	Reset();
	Push(Pose);
}

const FCYGTrackedPose& FCYGPoseInterpolator::At(int32 IndexFromOldest) const
{
	return Poses[(Head - Num + IndexFromOldest + Capacity) % Capacity];
}

FCYGTrackedPose FCYGPoseInterpolator::Sample(double RequestedSeconds, double MaxExtrapolationSeconds) const
{
	if (Num == 0)
	{
		return FCYGTrackedPose();
	}

	if (RequestedSeconds >= Newest().TimeSeconds)
	{
		FCYGTrackedPose coasting = Newest();
		coasting.Location = Newest().Location + Newest().Velocity * FMath::Min(RequestedSeconds - Newest().TimeSeconds, MaxExtrapolationSeconds);
		coasting.TimeSeconds = RequestedSeconds;

		return coasting;
	}

	if (RequestedSeconds <= Oldest().TimeSeconds)
	{
		return Oldest();
	}

	for (int32 Index = 0; Index < Num - 1; ++Index)
	{
		const FCYGTrackedPose& First = At(Index);
		const FCYGTrackedPose& Second = At(Index + 1);

		if (RequestedSeconds < First.TimeSeconds || RequestedSeconds > Second.TimeSeconds)
		{
			continue;
		}

		const double Span = Second.TimeSeconds - First.TimeSeconds;

		if (Span <= KINDA_SMALL_NUMBER)
		{
			return Second;
		}

		const double T = (RequestedSeconds - First.TimeSeconds) / Span;
		const double TSquare = T * T;
		const double TCube = TSquare * T;

		FCYGTrackedPose Result;
		Result.TimeSeconds = RequestedSeconds;
		Result.Location = First.Location * (2.0 * TCube - 3.0 * TSquare + 1.0)
			+ First.Velocity * (TSquare * T - 2.0 * TSquare + T) * Span
			+ Second.Location * (-2.0 * TCube + 3.0 * TSquare)
			+ Second.Velocity * (TCube - TSquare) * Span;

		// Shortest arc, blended linearly: the ends are a network tick apart, so the
		// difference is small and a full geodesic would cost a square root per frame.
		Result.Rotation = FQuat::FastLerp(First.Rotation, Second.Rotation, static_cast<float>(T)).GetNormalized();
		Result.Velocity = First.Velocity + (Second.Velocity - First.Velocity) * T;
		Result.AngularVelocity = Second.AngularVelocity;

		return Result;
	}

	return Newest();
}
