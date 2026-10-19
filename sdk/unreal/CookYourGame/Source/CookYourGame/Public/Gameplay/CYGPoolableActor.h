// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "UObject/Interface.h"
#include "CYGPoolableActor.generated.h"

/**
 * The two moments a recycled actor gets instead of BeginPlay and EndPlay.
 *
 * A pooled actor is never destroyed, so the engine has no reason to run its
 * play-state again. Anything a game wants to reset on reuse belongs in these
 * hooks rather than in BeginPlay.
 */
UINTERFACE(MinimalAPI, Blueprintable)
class UCYGPoolableActor : public UInterface
{
	GENERATED_BODY()
};

class COOKYOURGAME_API ICYGPooledActor
{
	GENERATED_BODY()

public:
	/** Called as the actor leaves the pool, after its pose has been placed. */
	UFUNCTION(BlueprintNativeEvent, Category = "CookYourGame|Network")
	void OnCYGSpawnedFromPool();

	virtual void OnCYGSpawnedFromPool_Implementation() {}

	/** Called as the actor goes back in, before it is hidden. */
	UFUNCTION(BlueprintNativeEvent, Category = "CookYourGame|Network")
	void OnCYGReturnedToPool();

	virtual void OnCYGReturnedToPool_Implementation() {}
};
