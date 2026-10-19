// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"

/**
 * World poses cross the wire in metres and in the Unity axes: X right, Y up,
 * Z forward. Unreal measures in centimetres with X forward, Y right, Z up, so
 * the two differ by a relabelling of the same three directions. Both engines
 * are left-handed, which is why nothing here flips a sign.
 *
 * Only world poses come through this door. A vector inside an rpc payload stays
 * in the sender's axes, because no sdk can know what a game argument means.
 */
struct COOKYOURGAME_API FCYGCoordinateSpace
{
	static constexpr float CentimetresPerMetre = 100.0f;

	/** Unreal centimetres to wire metres. */
	static FVector ToWire(const FVector& Value)
	{
		return Relabel(Value) / CentimetresPerMetre;
	}

	/** Unreal rotation to wire rotation, components relabelled the same way. */
	static FQuat ToWire(const FQuat& Value)
	{
		return FQuat(Value.Y, Value.Z, Value.X, Value.W);
	}

	/** Wire metres to Unreal centimetres. */
	static FVector FromWire(const FVector& Value)
	{
		return Unrelabel(Value) * CentimetresPerMetre;
	}

	/** Wire rotation to Unreal rotation. */
	static FQuat FromWire(const FQuat& Value)
	{
		return FQuat(Value.Z, Value.X, Value.Y, Value.W);
	}

	/** Axes only, for a direction or a scale factor, which carry no unit to convert. */
	static FVector Relabel(const FVector& Value)
	{
		return FVector(Value.Y, Value.Z, Value.X);
	}

	/** The inverse of Relabel, which is the other rotation of the same three axes. */
	static FVector Unrelabel(const FVector& Value)
	{
		return FVector(Value.Z, Value.X, Value.Y);
	}
};
