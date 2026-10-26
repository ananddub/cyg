// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "CYGEffects.generated.h"

class UCYGClient;
class UNiagaraSystem;
class USoundBase;

/**
 * Sounds and particles that need no actor.
 *
 * A muzzle flash does not deserve a view id, a delta stream and a replicated
 * transform: it is one sentence said to the room. The message rides a room event
 * under the name CYG_FX, so a server that only relays events carries it, and the
 * asset is named by a number from UCYGAssetRegistry rather than a path, so a
 * renamed asset cannot split a room in two.
 *
 * The sender plays its own effect immediately and ignores the echo.
 */
UCLASS()
class COOKYOURGAME_API UCYGEffects : public UObject
{
	GENERATED_BODY()

public:
	/** Listens to a client's room events; safe to call again after a reconnect. */
	void AttachTo(UCYGClient* InClient);

	void Detach();

	/** Plays a sound at a world position for everyone in the room. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network", meta = (ClampMin = "0.0"))
	bool PlaySoundAtLocation(USoundBase* Sound, const FVector& Location, float VolumeMultiplier = 1.0f, float PitchMultiplier = 1.0f);

	/** Spawns a particle system at a world position for everyone in the room. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network", meta = (ClampMin = "0.01"))
	bool PlayEffectAtLocation(UNiagaraSystem* System, const FVector& Location, FRotator Rotation, float Scale = 1.0f);

private:
	UFUNCTION()
	void HandleRoomEvent(const FString& EventName, const TArray<uint8>& Body);

	/** Broadcasts a message this object has already played locally. */
	bool Broadcast(uint8 Kind, uint16 AssetId, const TArray<uint8>& Tail);

	UPROPERTY()
	TObjectPtr<UCYGClient> Client;

	/** A sound and a particle system are different lengths, so the reader needs the kind. */
	enum EKYGEffectKind : uint8
	{
		Sound = 0,
		Particle = 1
	};
};
