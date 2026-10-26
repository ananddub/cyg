// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Engine/DataAsset.h"
#include "CYGClassRegistry.generated.h"

class AActor;

/** One actor class and the 16-bit number every peer knows it by. */
USTRUCT(BlueprintType)
struct FCYGNetworkedClass
{
	GENERATED_BODY()

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame")
	TSubclassOf<AActor> ActorClass;

	/** Wire identity: every client must resolve this id to the same class. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame", meta = (ClampMin = "0", UIMax = "65534"))
	int32 NetworkId = 0;
};

/**
 * Class id table shared by the whole room.
 *
 * Ids are assigned here rather than derived from a name hash so a renamed
 * blueprint keeps its wire identity, and two projects cannot drift apart
 * without the mismatch showing up as a failed spawn instead of a wrong actor.
 */
UCLASS(BlueprintType)
class COOKYOURGAME_API UCYGClassRegistry : public UDataAsset
{
	GENERATED_BODY()

public:
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame", meta = (TitleProperty = "ActorClass"))
	TArray<FCYGNetworkedClass> NetworkedClasses;

	virtual void PostLoad() override;

	/** Publishes this asset's table as the active one. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void Activate();

	/** Registers one class under one id, replacing any earlier entry. */
	static void Register(TSubclassOf<AActor> ActorClass, uint16 NetworkId);

	/** Class a peer asked to spawn; null when the id is unknown here. */
	static UClass* FindClass(uint16 NetworkId);

	/** Wire id of a class, when it was registered. */
	static bool FindId(const UClass* ActorClass, uint16& OutNetworkId);

	/** Empties the table, for a hard restart of the networking layer. */
	static void Clear();

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	static int32 Count();
};
