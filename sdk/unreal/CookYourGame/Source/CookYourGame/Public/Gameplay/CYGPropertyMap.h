// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "CYGPropertyMap.generated.h"

class FCYGBitReader;
class FCYGBitWriter;

/** What one named property holds. */
UENUM(BlueprintType)
enum class ECYGPropertyType : uint8
{
	None		UMETA(DisplayName = "Empty"),
	Bool		UMETA(DisplayName = "Bool"),
	Int			UMETA(DisplayName = "Integer"),
	Float		UMETA(DisplayName = "Float"),
	Text		UMETA(DisplayName = "Text"),
	Vector		UMETA(DisplayName = "Vector"),
	Rotation	UMETA(DisplayName = "Rotation"),
};

/** A single synced property value. */
USTRUCT(BlueprintType)
struct FCYGPropertyValue
{
	GENERATED_BODY()

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	ECYGPropertyType Type = ECYGPropertyType::None;

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	bool bBoolValue = false;

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	int64 IntValue = 0;

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	float FloatValue = 0.f;

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	FString TextValue;

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	FVector VectorValue = FVector::ZeroVector;

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	FQuat RotationValue = FQuat::Identity;

	bool operator==(const FCYGPropertyValue& Other) const;
	bool operator!=(const FCYGPropertyValue& Other) const { return !(*this == Other); }
};

/**
 * Named, typed state that travels with a view without registering a sync var.
 *
 * Use it for scores, flags and lobby properties: values you want to read by
 * name from Blueprint, and that a late joiner should receive in full.
 *
 * Wire format: [count u8] then per dirty key [name string][tag u8][value].
 */
USTRUCT(BlueprintType)
struct COOKYOURGAME_API FCYGPropertyMap
{
	GENERATED_BODY()

	UPROPERTY(BlueprintReadOnly, Category = "CookYourGame")
	TMap<FString, FCYGPropertyValue> Values;

	/** Keys changed locally since the last pack. */
	TSet<FString> DirtyKeys;

	void SetBool(const FString& Key, bool Value);
	void SetInt(const FString& Key, int64 Value);
	void SetFloat(const FString& Key, float Value);
	void SetText(const FString& Key, const FString& Value);
	void SetVector(const FString& Key, const FVector& Value);
	void SetRotation(const FString& Key, const FQuat& Value);

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void SetProperty(const FString& Key, FCYGPropertyValue Value);

	/** Writes without dirtying: state that arrived from the room. */
	void ApplyRemote(const FString& Key, const FCYGPropertyValue& Value);

	bool TryGet(const FString& Key, FCYGPropertyValue& OutValue) const;

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	bool GetBool(const FString& Key) const;

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	int64 GetInt(const FString& Key) const;

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	float GetFloat(const FString& Key) const;

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	FString GetText(const FString& Key) const;

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	FVector GetVector(const FString& Key) const;

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	FQuat GetRotation(const FString& Key) const;

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	bool Contains(const FString& Key) const;

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void Remove(const FString& Key);

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void Clear();

	bool HasChanges() const { return DirtyKeys.Num() > 0; }

	/** Packs the dirty keys and clears them. */
	void SerializeChanges(FCYGBitWriter& Target);

	/** Applies a block written by SerializeChanges. */
	void DeserializeChanges(FCYGBitReader& Source);

	/** Packs every key, used for the state a late joiner needs. */
	void SerializeAll(FCYGBitWriter& Target) const;

	/** Reads a block written by SerializeAll or SerializeChanges. */
	void DeserializeBlock(FCYGBitReader& Source);
};
