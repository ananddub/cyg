// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"

/**
 * The two shape tests a rewind query needs, with nothing engine-side in them.
 *
 * A live game would ask the physics scene what a ray hit, but a hit has to be
 * judged against the world as it was when the shot was fired, and the scene only
 * holds the present. So these take a shape as data and solve it directly, which
 * also means they can be tested without a physics scene, a level or an actor.
 *
 * Both work in whatever unit the caller's vectors are in and return the distance
 * along a unit direction, plus the surface normal at that point.
 */
namespace CYGRewindMath
{
	/** Nearest entry into a sphere, or the start of the ray when it began inside. */
	inline bool TryRaySphere(const FVector& Origin, const FVector& Direction, const FVector& Center,
		double Radius, double MaxDistance, double& OutDistance, FVector& OutNormal)
	{
		constexpr double Epsilon = 1e-6;

		const FVector ToCenter = Origin - Center;
		const double AlongAxis = FVector::DotProduct(Direction, ToCenter);
		const double Constant = FVector::DotProduct(ToCenter, ToCenter) - Radius * Radius;
		const double Discriminant = AlongAxis * AlongAxis - Constant;

		if (Discriminant < 0.0)
		{
			return false;
		}

		// The near root is the entry point. It can only be negative for two reasons, and they
		// need different answers: the shot started inside the shape, or the shape is behind
		// the shooter. Constant carries the sign that tells them apart, being negative exactly
		// when the origin is inside the radius.
		double Distance = -AlongAxis - FMath::Sqrt(Discriminant);

		if (Distance < 0.0)
		{
			if (Constant >= 0.0)
			{
				return false;
			}

			Distance = 0.0;
		}

		if (Distance > MaxDistance)
		{
			return false;
		}

		const FVector Radial = (Origin + Direction * Distance) - Center;
		const double RadialSquared = FVector::DotProduct(Radial, Radial);

		OutDistance = Distance;
		OutNormal = RadialSquared > Epsilon ? Radial / FMath::Sqrt(RadialSquared) : -Direction;

		return true;
	}

	/**
	 * Nearest entry into an axis-aligned box given as a centre and half extents.
	 *
	 * Slab method: clip the parametric ray against each axis's pair of planes and keep
	 * the interval that survives all three. The axis that last pushed the near side
	 * outward is the face the ray came through, which is what the normal is for.
	 */
	inline bool TryRayBox(const FVector& Origin, const FVector& Direction, const FVector& Center,
		const FVector& Extents, double MaxDistance, double& OutDistance, FVector& OutNormal)
	{
		constexpr double Epsilon = 1e-6;

		const FVector Minimum = Center - Extents;
		const FVector Maximum = Center + Extents;

		double Entering = 0.0;
		double Leaving = MaxDistance;

		int32 EntryAxis = -1;
		bool bEntryFromMinimum = false;

		for (int32 Axis = 0; Axis < 3; ++Axis)
		{
			const double Start = Origin[Axis];
			const double Step = Direction[Axis];

			if (FMath::Abs(Step) < Epsilon)
			{
				// Parallel to the slab: either inside it for good, or never in it.
				if (Start < Minimum[Axis] || Start > Maximum[Axis])
				{
					return false;
				}

				continue;
			}

			const double InvStep = 1.0 / Step;
			double EntryPlane = (Minimum[Axis] - Start) * InvStep;
			double ExitPlane = (Maximum[Axis] - Start) * InvStep;

			const bool bEntryIsLowSide = EntryPlane < ExitPlane;

			if (!bEntryIsLowSide)
			{
				FMath::Swap(EntryPlane, ExitPlane);
			}

			if (EntryPlane > Entering)
			{
				Entering = EntryPlane;
				EntryAxis = Axis;
				bEntryFromMinimum = bEntryIsLowSide;
			}

			if (ExitPlane < Leaving)
			{
				Leaving = ExitPlane;
			}

			if (Entering > Leaving)
			{
				return false;
			}
		}

		if (Entering > MaxDistance)
		{
			return false;
		}

		OutDistance = Entering;

		if (EntryAxis < 0)
		{
			OutNormal = -Direction;
		}
		else
		{
			const double Sign = bEntryFromMinimum ? -1.0 : 1.0;
			OutNormal = FVector(EntryAxis == 0 ? Sign : 0.0, EntryAxis == 1 ? Sign : 0.0, EntryAxis == 2 ? Sign : 0.0);
		}

		return true;
	}
}
