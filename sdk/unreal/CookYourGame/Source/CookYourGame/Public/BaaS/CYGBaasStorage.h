// Copyright CookYourGame. All rights reserved.

#pragma once

#include "BaaS/CYGBaasHttp.h"
#include "CoreMinimal.h"
#include "Templates/Function.h"

class FJsonObject;

/** One stored document, as the server keeps it. */
struct FCYGBaasStorageObject
{
	FString Collection;
	FString Key;
	FString OwnerId;
	FString Value;
	FString Version;
	FString CreateTime;
	FString UpdateTime;

	int32 PermissionRead = 0;
	int32 PermissionWrite = 0;
};

/** [found, value] The value is empty when the document is not there. */
using FCYGBaasValueResponse = TFunction<void(bool, const FString&)>;

/** [succeeded, objects] */
using FCYGBaasObjectListResponse = TFunction<void(bool, const TArray<FCYGBaasStorageObject>&)>;

/**
 * Document storage for saves, progression and world state.
 *
 * A document is json the SDK passes through untouched: the server stores it as
 * text and version-checks it, while what it means is the game's business. Reads
 * name a player by id, and an empty id means the signed-in player, which is why
 * these calls have to follow a login rather than run before one.
 */
class COOKYOURGAME_API FCYGBaasStorage
{
public:
	/** The one document a collection and key name, for a player or the signed-in one. */
	static void Read(const FString& Collection, const FString& Key, const FCYGBaasValueResponse& Done, const FString& OwnerId = FString());

	/** Writes a document, creating it when it is missing. ValueJson is stored as sent. */
	static void Write(const FString& Collection, const FString& Key, const FString& ValueJson, const FCYGBaasBoolResponse& Done, int32 PermissionRead = 1, int32 PermissionWrite = 1);

	static void Delete(const FString& Collection, const FString& Key, const FCYGBaasBoolResponse& Done);

	/** Documents of a collection, whoever's they are, up to Limit. */
	static void ListCollection(const FString& Collection, const FCYGBaasObjectListResponse& Done, int32 Limit = 100);

private:
	/** A reply that carries a list of documents, decoded. */
	static void ReadObjects(const FString& Body, TArray<FCYGBaasStorageObject>& OutObjects);

	/** One document as the server describes it. */
	static FCYGBaasStorageObject ParseObject(const TSharedPtr<FJsonObject>& Object);

	/** The player a call is about: the one named, or the signed-in one. */
	static FString OwnerOrSelf(const FString& OwnerId);
};
