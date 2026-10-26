// Copyright CookYourGame. All rights reserved.

#include "BaaS/CYGBaasInventory.h"

#include "CookYourGame.h"
#include "CYGBaasJson.h"

namespace
{
	/** The server's answer, kept so a screen can be drawn without another call. */
	TArray<FCYGBaasInventoryItem> Cached;
}

FOnCYGInventoryChanged FCYGBaasInventory::OnChanged;

const TArray<FCYGBaasInventoryItem>& FCYGBaasInventory::Items()
{
	return Cached;
}

void FCYGBaasInventory::GetInventory(const FCYGBaasInventoryResponse& Done)
{
	FCYGBaasHttp::Get(TEXT("/v2/inventory"),
		[Done](bool bSuccess, int32 StatusCode, const FString& Body)
		{
			TArray<FCYGBaasInventoryItem> Fetched;

			if (bSuccess)
			{
				FCYGBaasInventory::ParseItems(Body, Fetched);
				FCYGBaasInventory::Publish(MoveTemp(Fetched));
			}
			else
			{
				UE_LOG(LogCookYourGame, Error, TEXT("Reading the inventory failed (%d): %s"), StatusCode, *CYGBaasJson::ReadError(CYGBaasJson::Parse(Body)));
			}

			if (Done)
			{
				Done(bSuccess, FCYGBaasInventory::Items());
			}
		});
}

void FCYGBaasInventory::Equip(const FString& InstanceId, int32 SlotIndex, const FCYGBaasBoolResponse& Done)
{
	const TSharedRef<FJsonObject> Body = MakeShared<FJsonObject>();
	Body->SetStringField(TEXT("instance_id"), InstanceId);
	Body->SetNumberField(TEXT("slot_index"), SlotIndex);

	FCYGBaasHttp::Post(TEXT("/v2/inventory/equip"), CYGBaasJson::Write(Body),
		[Done](bool, int32 StatusCode, const FString& Reply)
		{
			FCYGBaasInventory::HandleOperationReply(Reply, StatusCode, TEXT("equip"), Done);
		});
}

void FCYGBaasInventory::Consume(const FString& InstanceId, const FCYGBaasBoolResponse& Done, int32 Quantity)
{
	const TSharedRef<FJsonObject> Body = MakeShared<FJsonObject>();
	Body->SetStringField(TEXT("instance_id"), InstanceId);
	Body->SetNumberField(TEXT("quantity"), FMath::Max(1, Quantity));

	FCYGBaasHttp::Post(TEXT("/v2/inventory/consume"), CYGBaasJson::Write(Body),
		[Done](bool, int32 StatusCode, const FString& Reply)
		{
			FCYGBaasInventory::HandleOperationReply(Reply, StatusCode, TEXT("consume"), Done);
		});
}

void FCYGBaasInventory::Grant(const FString& ItemId, const FCYGBaasBoolResponse& Done, int32 Quantity, int32 SlotIndex)
{
	const TSharedRef<FJsonObject> Body = MakeShared<FJsonObject>();
	Body->SetStringField(TEXT("item_id"), ItemId);
	Body->SetNumberField(TEXT("quantity"), FMath::Max(1, Quantity));
	Body->SetNumberField(TEXT("slot_index"), SlotIndex);

	FCYGBaasHttp::Post(TEXT("/v2/inventory/grant"), CYGBaasJson::Write(Body),
		[Done](bool, int32 StatusCode, const FString& Reply)
		{
			FCYGBaasInventory::HandleOperationReply(Reply, StatusCode, TEXT("grant"), Done);
		});
}

const FCYGBaasInventoryItem* FCYGBaasInventory::FindInSlot(int32 SlotIndex)
{
	return Cached.FindByPredicate([SlotIndex](const FCYGBaasInventoryItem& Item) { return Item.SlotIndex == SlotIndex; });
}

const FCYGBaasInventoryItem* FCYGBaasInventory::FindByInstanceId(const FString& InstanceId)
{
	return Cached.FindByPredicate([&InstanceId](const FCYGBaasInventoryItem& Item) { return Item.InstanceId == InstanceId; });
}

void FCYGBaasInventory::ClearCache()
{
	Cached.Reset();
	OnChanged.Broadcast(Cached);
}

void FCYGBaasInventory::Publish(TArray<FCYGBaasInventoryItem>&& NewItems)
{
	Cached = MoveTemp(NewItems);

	// A copy of the broadcast list, because a listener is allowed to ask for the
	// inventory again and that would replace the array being handed to it.
	TArray<FCYGBaasInventoryItem> Snapshot = Cached;
	OnChanged.Broadcast(Snapshot);
}

void FCYGBaasInventory::HandleOperationReply(const FString& Body, int32 StatusCode, const TCHAR* Operation, const FCYGBaasBoolResponse& Done)
{
	const TSharedPtr<FJsonObject> Object = CYGBaasJson::Parse(Body);
	const bool bTook = StatusCode >= 200 && StatusCode < 300 && CYGBaasJson::ReadBool(Object, TEXT("success"));

	if (!bTook)
	{
		UE_LOG(LogCookYourGame, Error, TEXT("The %s was refused (%d): %s"), Operation, StatusCode, *CYGBaasJson::ReadError(Object));

		if (Done)
		{
			Done(false);
		}

		return;
	}

	// The numbers only the server knows changed, so the answer is reported after the
	// refresh that reads them back.
	GetInventory([Done](bool, const TArray<FCYGBaasInventoryItem>&)
	{
		if (Done)
		{
			Done(true);
		}
	});
}

void FCYGBaasInventory::ParseItems(const FString& Body, TArray<FCYGBaasInventoryItem>& OutItems)
{
	const TSharedPtr<FJsonObject> Object = CYGBaasJson::Parse(Body);

	for (const TSharedPtr<FJsonValue>& Element : CYGBaasJson::ReadArray(Object, TEXT("items")))
	{
		OutItems.Add(ParseItem(CYGBaasJson::ReadElement(Element)));
	}
}

FCYGBaasInventoryItem FCYGBaasInventory::ParseItem(const TSharedPtr<FJsonObject>& Object)
{
	FCYGBaasInventoryItem Item;

	if (!Object.IsValid())
	{
		return Item;
	}

	Item.InstanceId = CYGBaasJson::ReadText(Object, TEXT("instance_id"));
	Item.OwnerId = CYGBaasJson::ReadText(Object, TEXT("user_id"));
	Item.ItemId = CYGBaasJson::ReadText(Object, TEXT("item_id"));
	Item.Metadata = CYGBaasJson::ReadText(Object, TEXT("metadata"));
	Item.CreatedAt = CYGBaasJson::ReadText(Object, TEXT("created_at"));
	Item.UpdatedAt = CYGBaasJson::ReadText(Object, TEXT("updated_at"));
	Item.Quantity = static_cast<int32>(CYGBaasJson::ReadInt(Object, TEXT("quantity")));

	const int64 Slot = CYGBaasJson::ReadInt(Object, TEXT("slot_index"));
	Item.SlotIndex = Object->HasField(TEXT("slot_index")) ? static_cast<int32>(Slot) : -1;

	return Item;
}
