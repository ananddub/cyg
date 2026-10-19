// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Engine/DataAsset.h"
#include "CYGAssetRegistry.generated.h"

class UNiagaraSystem;
class USoundBase;

/** One sound and the 16-bit number every peer knows it by. */
USTRUCT(BlueprintType)
struct FCYGNetworkedSound
{
	GENERATED_BODY()

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame")
	TObjectPtr<USoundBase> Sound;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame", meta = (ClampMin = "0", UIMax = "65534"))
	int32 NetworkId = 0;
};

/** One particle system and the 16-bit number every peer knows it by. */
USTRUCT(BlueprintType)
struct FCYGNetworkedEffect
{
	GENERATED_BODY()

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame")
	TObjectPtr<UNiagaraSystem> Effect;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame", meta = (ClampMin = "0", UIMax = "65534"))
	int32 NetworkId = 0;
};

/**
 * Asset id table for effects that play without an actor.
 *
 * The same argument as the class registry: an authored number survives a rename,
 * and two builds that disagree about a table fail loudly as a missing effect
 * instead of playing the wrong one.
 */
UCLASS(BlueprintType)
class COOKYOURGAME_API UCYGAssetRegistry : public UDataAsset
{
	GENERATED_BODY()

public:
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame", meta = (TitleProperty = "Sound"))
	TArray<FCYGNetworkedSound> Sounds;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame", meta = (TitleProperty = "Effect"))
	TArray<FCYGNetworkedEffect> Effects;

	virtual void PostLoad() override;

	/** Publishes this asset's tables as the active ones. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void Activate();

	static void RegisterSound(USoundBase* Sound, uint16 NetworkId);

	static void RegisterEffect(UNiagaraSystem* Effect, uint16 NetworkId);

	/** Sound a peer asked for, null when the id is unknown here. */
	static USoundBase* FindSound(uint16 NetworkId);

	static UNiagaraSystem* FindEffect(uint16 NetworkId);

	static bool FindSoundId(const USoundBase* Sound, uint16& OutNetworkId);

	static bool FindEffectId(const UNiagaraSystem* Effect, uint16& OutNetworkId);

	/** Empties both tables, for a hard restart of the networking layer. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	static void Clear();
};
