// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"

/**
 * Reading and writing control-plane json, module internal.
 *
 * Every account endpoint answers with an object a caller usually wants two or
 * three fields out of, and the fields a server leaves out are as common as the
 * ones it sends, so the tolerant readers live here once instead of in each BaaS
 * file. Each reader looks at the value's type before asking for it, so a field the
 * server sent as a number rather than a string is still read, and nothing is ever
 * handed the wrong accessor.
 *
 * UCYGClient keeps its own private copies for the seat reply; that path predates
 * this header and sits on the connection's critical path.
 */
namespace CYGBaasJson
{
	/** Parsed object, null when the text is not json or is not an object. */
	inline TSharedPtr<FJsonObject> Parse(const FString& Text)
	{
		TSharedPtr<FJsonObject> Object;
		const TSharedRef<TJsonReader<>> Reader = TJsonReaderFactory<>::Create(Text);

		if (!FJsonSerializer::Deserialize(Reader, Object) || !Object.IsValid())
		{
			return nullptr;
		}

		return Object;
	}

	/** Serialises an object the caller built field by field. */
	inline FString Write(const TSharedRef<FJsonObject>& Object)
	{
		FString Text;
		const TSharedRef<TJsonWriter<>> Writer = TJsonWriterFactory<>::Create(&Text);

		return FJsonSerializer::Serialize(Object, Writer) ? Text : FString();
	}

	inline TSharedRef<FJsonObject> NewObject()
	{
		return MakeShared<FJsonObject>();
	}

	inline void Put(FJsonObject& Object, const TCHAR* Field, const FString& Value)
	{
		Object.SetStringField(Field, Value);
	}

	inline void Put(FJsonObject& Object, const TCHAR* Field, const TCHAR* Value)
	{
		Object.SetStringField(Field, Value);
	}

	inline void Put(FJsonObject& Object, const TCHAR* Field, int64 Value)
	{
		Object.SetNumberField(Field, static_cast<double>(Value));
	}

	inline void Put(FJsonObject& Object, const TCHAR* Field, int32 Value)
	{
		Object.SetNumberField(Field, static_cast<double>(Value));
	}

	inline void Put(FJsonObject& Object, const TCHAR* Field, bool Value)
	{
		Object.SetBoolField(Field, Value);
	}

	inline void Put(FJsonObject& Object, const TCHAR* Field, const TArray<TSharedPtr<FJsonValue>>& Values)
	{
		Object.SetArrayField(Field, Values);
	}

	inline void Put(FJsonObject& Object, const TCHAR* Field, const TSharedPtr<FJsonObject>& Value)
	{
		Object.SetObjectField(Field, Value);
	}

	/** Wraps finished objects as the elements of an array field. */
	inline TArray<TSharedPtr<FJsonValue>> Wrap(const TArray<TSharedRef<FJsonObject>>& Objects)
	{
		TArray<TSharedPtr<FJsonValue>> Elements;

		for (const TSharedRef<FJsonObject>& Object : Objects)
		{
			Elements.Add(MakeShared<FJsonValueObject>(Object));
		}

		return Elements;
	}

	inline const TSharedPtr<FJsonValue>* Find(const TSharedPtr<FJsonObject>& Object, const TCHAR* Field)
	{
		if (!Object.IsValid())
		{
			return nullptr;
		}

		const TSharedPtr<FJsonValue>* Value = Object->TryGetField(Field);

		return Value != nullptr && (*Value).IsValid() ? Value : nullptr;
	}

	/**
	 * Text of a field, whichever type it arrived as.
	 *
	 * The readers check the type before asking for it because the json accessors
	 * complain when handed the wrong one, and this plane answers the same field as a
	 * string or a number depending on the endpoint: scores and ranks come back as
	 * decimal strings, timestamps as epoch numbers.
	 */
	inline FString ReadText(const TSharedPtr<FJsonObject>& Object, const TCHAR* Field)
	{
		const TSharedPtr<FJsonValue>* Value = Find(Object, Field);

		if (Value == nullptr)
		{
			return FString();
		}

		switch ((*Value)->Type())
		{
		case EJson::String:
			return (*Value)->AsString();

		case EJson::Number:
		{
			const double Number = (*Value)->AsNumber();

			// A whole number keeps its digits: an exponent would corrupt an id.
			return Number == FMath::FloorToDouble(Number)
				? FString::Printf(TEXT("%.0f"), Number)
				: FString::Printf(TEXT("%g"), Number);
		}

		case EJson::Boolean:
			return (*Value)->AsBool() ? FString(TEXT("true")) : FString(TEXT("false"));

		default:
			return FString();
		}
	}

	/** Integer of a field, whether it arrived as a number or as digits in a string. */
	inline int64 ReadInt(const TSharedPtr<FJsonObject>& Object, const TCHAR* Field)
	{
		const TSharedPtr<FJsonValue>* Value = Find(Object, Field);

		if (Value == nullptr)
		{
			return 0;
		}

		switch ((*Value)->Type())
		{
		case EJson::Number:
			return static_cast<int64>((*Value)->AsNumber());

		case EJson::String:
			return FCString::Atoi64(*(*Value)->AsString());

		case EJson::Boolean:
			return (*Value)->AsBool() ? 1 : 0;

		default:
			return 0;
		}
	}

	inline bool ReadBool(const TSharedPtr<FJsonObject>& Object, const TCHAR* Field)
	{
		const TSharedPtr<FJsonValue>* Value = Find(Object, Field);

		if (Value == nullptr)
		{
			return false;
		}

		switch ((*Value)->Type())
		{
		case EJson::Boolean:
			return (*Value)->AsBool();

		case EJson::Number:
			return (*Value)->AsNumber() != 0.0;

		case EJson::String:
			return (*Value)->AsString().Equals(TEXT("true"), ESearchCase::IgnoreCase)
				|| FCString::Atoi64(*(*Value)->AsString()) != 0;

		default:
			return false;
		}
	}

	/**
	 * Elements of an array field, empty when the field is missing or is not an array.
	 *
	 * Returned by value on purpose: an accessor that handed back a pointer into the
	 * parsed document would outlive nothing here, because the document is a temporary
	 * at every one of these call sites.
	 */
	inline TArray<TSharedPtr<FJsonValue>> ReadArray(const TSharedPtr<FJsonObject>& Object, const TCHAR* Field)
	{
		const TSharedPtr<FJsonValue>* Value = Find(Object, Field);

		if (Value == nullptr || (*Value)->Type() != EJson::Array)
		{
			return TArray<TSharedPtr<FJsonValue>>();
		}

		return (*Value)->AsArray();
	}

	/** One nested object, null when the field is missing or is not an object. */
	inline TSharedPtr<FJsonObject> ReadObject(const TSharedPtr<FJsonObject>& Object, const TCHAR* Field)
	{
		const TSharedPtr<FJsonValue>* Value = Find(Object, Field);

		if (Value == nullptr || (*Value)->Type() != EJson::Object)
		{
			return nullptr;
		}

		return (*Value)->AsObject();
	}

	/** One element of an array field, null when the element is not an object. */
	inline TSharedPtr<FJsonObject> ReadElement(const TSharedPtr<FJsonValue>& Element)
	{
		if (!Element.IsValid() || Element->Type() != EJson::Object)
		{
			return nullptr;
		}

		return Element->AsObject();
	}

	/** The reason a call failed, whichever field the server put it in. */
	inline FString ReadError(const TSharedPtr<FJsonObject>& Object)
	{
		FString Reason = ReadText(Object, TEXT("message"));

		if (Reason.IsEmpty())
		{
			Reason = ReadText(Object, TEXT("error"));
		}

		if (Reason.IsEmpty())
		{
			Reason = ReadText(ReadObject(Object, TEXT("httpError")), TEXT("message"));
		}

		return Reason;
	}
}
