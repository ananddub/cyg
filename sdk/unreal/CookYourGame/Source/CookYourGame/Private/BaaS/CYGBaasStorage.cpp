// Copyright CookYourGame. All rights reserved.

#include "BaaS/CYGBaasStorage.h"

#include "BaaS/CYGBaasAuth.h"
#include "CookYourGame.h"
#include "Core/CYGSettings.h"
#include "CYGBaasJson.h"

namespace
{
	/** Builds the {object_ids: [...]} or {objects: [...]} wrapper a call takes. */
	FString Wrap(const TCHAR* Field, const TArray<TSharedRef<FJsonObject>>& Objects)
	{
		const TSharedRef<FJsonObject> Body = MakeShared<FJsonObject>();
		Body->SetArrayField(Field, CYGBaasJson::Wrap(Objects));

		return CYGBaasJson::Write(Body);
	}

	TSharedRef<FJsonObject> MakeDocumentId(const FString& Collection, const FString& Key, const FString& OwnerId)
	{
		const TSharedRef<FJsonObject> Object = MakeShared<FJsonObject>();
		Object->SetStringField(TEXT("collection"), Collection);
		Object->SetStringField(TEXT("key"), Key);

		if (!OwnerId.IsEmpty())
		{
			Object->SetStringField(TEXT("user_id"), OwnerId);
		}

		return Object;
	}

	/** Only the status line, for a write whose body says nothing the caller needs. */
	bool TookTheCall(bool bSuccess, int32 StatusCode, const FString& Body)
	{
		if (!bSuccess)
		{
			UE_LOG(LogCookYourGame, Error, TEXT("A storage call failed (%d): %s"), StatusCode, *CYGBaasJson::ReadError(CYGBaasJson::Parse(Body)));
		}

		return bSuccess;
	}
}

void FCYGBaasStorage::Read(const FString& Collection, const FString& Key, const FCYGBaasValueResponse& Done, const FString& OwnerId)
{
	const TArray<TSharedRef<FJsonObject>> Ids{ MakeDocumentId(Collection, Key, OwnerOrSelf(OwnerId)) };

	FCYGBaasHttp::Post(TEXT("/v2/storage"), Wrap(TEXT("object_ids"), Ids),
		[Done](bool bSuccess, int32 StatusCode, const FString& Body)
		{
			TArray<FCYGBaasStorageObject> Objects;
			FCYGBaasStorage::ReadObjects(Body, Objects);

			if (!TookTheCall(bSuccess, StatusCode, Body) || Objects.Num() == 0)
			{
				if (Done)
				{
					Done(false, FString());
				}

				return;
			}

			if (Done)
			{
				Done(true, Objects[0].Value);
			}
		});
}

void FCYGBaasStorage::Write(const FString& Collection, const FString& Key, const FString& ValueJson, const FCYGBaasBoolResponse& Done, int32 PermissionRead, int32 PermissionWrite)
{
	const TSharedRef<FJsonObject> Document = MakeShared<FJsonObject>();
	Document->SetStringField(TEXT("collection"), Collection);
	Document->SetStringField(TEXT("key"), Key);
	Document->SetStringField(TEXT("value"), ValueJson);
	Document->SetNumberField(TEXT("permission_read"), PermissionRead);
	Document->SetNumberField(TEXT("permission_write"), PermissionWrite);

	const TArray<TSharedRef<FJsonObject>> Documents{ Document };

	FCYGBaasHttp::Put(TEXT("/v2/storage"), Wrap(TEXT("objects"), Documents),
		[Done](bool bSuccess, int32 StatusCode, const FString& Body)
		{
			if (Done)
			{
				Done(TookTheCall(bSuccess, StatusCode, Body));
			}
		});
}

void FCYGBaasStorage::Delete(const FString& Collection, const FString& Key, const FCYGBaasBoolResponse& Done)
{
	const TArray<TSharedRef<FJsonObject>> Ids{ MakeDocumentId(Collection, Key, OwnerOrSelf(FString())) };

	FCYGBaasHttp::Put(TEXT("/v2/storage/delete"), Wrap(TEXT("object_ids"), Ids),
		[Done](bool bSuccess, int32 StatusCode, const FString& Body)
		{
			if (Done)
			{
				Done(TookTheCall(bSuccess, StatusCode, Body));
			}
		});
}

void FCYGBaasStorage::ListCollection(const FString& Collection, const FCYGBaasObjectListResponse& Done, int32 Limit)
{
	const FString Path = FString::Printf(TEXT("/v2/storage/%s?limit=%d"), *UCYGSettings::EscapeSegment(Collection), FMath::Max(1, Limit));

	FCYGBaasHttp::Get(Path,
		[Done](bool bSuccess, int32 StatusCode, const FString& Body)
		{
			TArray<FCYGBaasStorageObject> Objects;
			FCYGBaasStorage::ReadObjects(Body, Objects);

			if (!TookTheCall(bSuccess, StatusCode, Body))
			{
				if (Done)
				{
					Done(false, Objects);
				}

				return;
			}

			if (Done)
			{
				Done(true, Objects);
			}
		});
}

void FCYGBaasStorage::ReadObjects(const FString& Body, TArray<FCYGBaasStorageObject>& OutObjects)
{
	OutObjects.Reset();

	const TSharedPtr<FJsonObject> Object = CYGBaasJson::Parse(Body);

	for (const TSharedPtr<FJsonValue>& Element : CYGBaasJson::ReadArray(Object, TEXT("objects")))
	{
		OutObjects.Add(ParseObject(CYGBaasJson::ReadElement(Element)));
	}
}

FCYGBaasStorageObject FCYGBaasStorage::ParseObject(const TSharedPtr<FJsonObject>& Object)
{
	FCYGBaasStorageObject Parsed;

	if (!Object.IsValid())
	{
		return Parsed;
	}

	Parsed.Collection = CYGBaasJson::ReadText(Object, TEXT("collection"));
	Parsed.Key = CYGBaasJson::ReadText(Object, TEXT("key"));
	Parsed.OwnerId = CYGBaasJson::ReadText(Object, TEXT("user_id"));
	Parsed.Value = CYGBaasJson::ReadText(Object, TEXT("value"));
	Parsed.Version = CYGBaasJson::ReadText(Object, TEXT("version"));
	Parsed.CreateTime = CYGBaasJson::ReadText(Object, TEXT("create_time"));
	Parsed.UpdateTime = CYGBaasJson::ReadText(Object, TEXT("update_time"));
	Parsed.PermissionRead = static_cast<int32>(CYGBaasJson::ReadInt(Object, TEXT("permission_read")));
	Parsed.PermissionWrite = static_cast<int32>(CYGBaasJson::ReadInt(Object, TEXT("permission_write")));

	return Parsed;
}

FString FCYGBaasStorage::OwnerOrSelf(const FString& OwnerId)
{
	return OwnerId.IsEmpty() ? FCYGBaasAuth::UserId() : OwnerId;
}
