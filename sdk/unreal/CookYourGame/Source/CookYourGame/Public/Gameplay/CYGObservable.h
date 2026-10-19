// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "UObject/Interface.h"
#include "CYGObservable.generated.h"

class FCYGBitReader;
class FCYGBitWriter;

/**
 * Anything a view can stream as part of its delta.
 *
 * Serialize and Deserialize are exact mirrors: the delta stream carries no
 * length prefixes, so a component must consume precisely the bytes it wrote,
 * in the same order.
 */
UINTERFACE(MinimalAPI)
class UCYGObservable : public UInterface
{
	GENERATED_BODY()
};

class COOKYOURGAME_API ICYGObservable
{
	GENERATED_BODY()

public:
	/** True when local state changed since the last pack. */
	virtual bool HasCYGDelta() const = 0;

	/** Appends this component's blob to the view delta. */
	virtual void SerializeCYG(FCYGBitWriter& Target) = 0;

	/** Consumes exactly what SerializeCYG produced. */
	virtual void DeserializeCYG(FCYGBitReader& Source) = 0;
};
