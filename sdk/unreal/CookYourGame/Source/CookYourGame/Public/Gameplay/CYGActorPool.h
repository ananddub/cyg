// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"

class AActor;
class UWorld;

/**
 * Recycled actors, keyed by class.
 *
 * Spawn and destroy is the most expensive thing a networked game does on the
 * hot path: a bullet, a hit effect and a picked-up item all live for a second
 * or two. A pooled class keeps its actor around hidden and inert between uses,
 * so the second spawn costs a transform write instead of a construction, an
 * archetype copy and a component registration.
 *
 * A recycled actor does not see BeginPlay again, which is what ICYGPooledActor
 * is for.
 */
struct COOKYOURGAME_API FCYGActorPool
{
	/** Idle actors kept per class once pooling is on. */
	static constexpr int32 DefaultCapacity = 128;

	/** Start recycling actors of this class; further spawns and releases go through the pool. */
	static void EnablePooling(UClass* Class, int32 Capacity = DefaultCapacity);

	/** Turn recycling off again; the class keeps whatever is already idle. */
	static void DisablePooling(UClass* Class);

	static bool IsPoolingEnabled(UClass* Class);

	/** Creates idle actors up front, so the first spawns never construct anything. */
	static void Prewarm(UWorld* World, UClass* Class, int32 Count);

	/** A pooled actor placed at the transform, or null when pooling is off for the class. */
	static AActor* Acquire(UWorld* World, UClass* Class, const FTransform& Transform);

	/**
	 * Takes an actor back. False means the caller still owns it, either because
	 * its class is not pooled or because the idle list is already full, and the
	 * actor is destroyed instead.
	 */
	static bool Release(AActor* Actor);

	/** Destroys the idle actors. A level change or a lost link makes them worthless. */
	static void Clear();

	static int32 IdleCount();
	static int32 ActiveCount();
	static int32 TotalCreated();

private:
	/** Hides an actor without destroying it, and undoes that on the way out. */
	static void SetInert(AActor* Actor, bool bInert);
};
