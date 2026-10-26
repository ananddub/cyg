// Copyright CookYourGame. All rights reserved.

#pragma once

#include "BaaS/CYGBaasHttp.h"
#include "CoreMinimal.h"
#include "Templates/Function.h"

class FJsonObject;

/** One item the server holds for a player. */
struct FCYGBaasInventoryItem
{
	FString InstanceId;
	FString OwnerId;
	FString ItemId;
	FString Metadata;
	FString CreatedAt;
	FString UpdatedAt;

	int32 Quantity = 0;

	/** -1 while the item sits in the bag rather than on a slot. */
	int32 SlotIndex = -1;
};

/** [succeeded, items] The list is the whole inventory, cached or empty. */
using FCYGBaasInventoryResponse = TFunction<void(bool, const TArray<FCYGBaasInventoryItem>&)>;

DECLARE_MULTICAST_DELEGATE_OneParam(FOnCYGInventoryChanged, const TArray<FCYGBaasInventoryItem>& /*Items*/);

/**
 * Server-authoritative items, equipment slots and consumption.
 *
 * The server owns the numbers: a client asks for an item to be consumed and takes
 * back whatever the answer says, which is why every operation refreshes the local
 * copy instead of editing it. The cache is a convenience for drawing a screen, not
 * a state the game may change.
 */
class COOKYOURGAME_API FCYGBaasInventory
{
public:
	/** Raised whenever the cached list is replaced by the server's answer. */
	static FOnCYGInventoryChanged OnChanged;

	/** The cached inventory; empty until the first fetch. */
	static const TArray<FCYGBaasInventoryItem>& Items();

	static void GetInventory(const FCYGBaasInventoryResponse& Done);

	static void Equip(const FString& InstanceId, int32 SlotIndex, const FCYGBaasBoolResponse& Done);

	static void Consume(const FString& InstanceId, const FCYGBaasBoolResponse& Done, int32 Quantity = 1);

	static void Grant(const FString& ItemId, const FCYGBaasBoolResponse& Done, int32 Quantity = 1, int32 SlotIndex = -1);

	/** The item on a slot, null when nothing is equipped there. */
	static const FCYGBaasInventoryItem* FindInSlot(int32 SlotIndex);

	static const FCYGBaasInventoryItem* FindByInstanceId(const FString& InstanceId);

	/** Forgets the cache, for a sign-out that must not show the last player's bag. */
	static void ClearCache();

private:
	static void ParseItems(const FString& Body, TArray<FCYGBaasInventoryItem>& OutItems);

	static FCYGBaasInventoryItem ParseItem(const TSharedPtr<FJsonObject>& Object);

	/**
	 * Reads the {success, message} an operation answers with, then refreshes.
	 * Done sees the operation's verdict, not the refresh's.
	 */
	static void HandleOperationReply(const FString& Body, int32 StatusCode, const TCHAR* Operation, const FCYGBaasBoolResponse& Done);

	static void Publish(TArray<FCYGBaasInventoryItem>&& NewItems);
};
