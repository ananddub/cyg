// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "CYGPlayer.generated.h"

/** One participant in the room, keyed by the short actor number peers address. */
USTRUCT(BlueprintType)
struct COOKYOURGAME_API FCYGPlayer
{
	GENERATED_BODY()

	/** Short id used on the wire; unique inside the room for this session. */
	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	int32 ActorNumber = 0;

	/** Session id issued by the matchmaker, empty for remote peers. */
	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	FString PlayerId;

	/** Name the participant connected with. */
	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	FString Nickname;

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	bool bIsLocal = false;

	/** True for the participant with the lowest actor number. */
	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	bool bIsMasterClient = false;

	/** Free-form properties carried with the participant. */
	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	TMap<FString, FString> CustomProperties;
};
