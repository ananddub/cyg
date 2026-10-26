// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGPropertyMap.h"

#include "CookYourGame.h"
#include "Serialization/CYGBitPacker.h"

bool FCYGPropertyValue::operator==(const FCYGPropertyValue& Other) const
{
	if (Type != Other.Type)
	{
		return false;
	}

	switch (Type)
	{
	case ECYGPropertyType::Bool:		return bBoolValue == Other.bBoolValue;
	case ECYGPropertyType::Int:			return IntValue == Other.IntValue;
	case ECYGPropertyType::Float:		return FloatValue == Other.FloatValue;
	case ECYGPropertyType::Text:		return TextValue == Other.TextValue;
	case ECYGPropertyType::Vector:		return VectorValue == Other.VectorValue;
	case ECYGPropertyType::Rotation:	return RotationValue.Equals(Other.RotationValue, KIND_SMALL_NUMBER);
	default:							return true;
	}
}

namespace
{
	/** Wire tag of a value type; peers agree on these numbers, not on the enum name. */
	uint8 TagOf(ECYGPropertyType Type)
	{
		switch (Type)
		{
		case ECYGPropertyType::Bool:		return 1;
		case ECYGPropertyType::Int:			return 2;
		case ECYGPropertyType::Float:		return 3;
		case ECYGPropertyType::Text:		return 4;
		case ECYGPropertyType::Vector:		return 5;
		case ECYGPropertyType::Rotation:	return 6;
		default:							return 0;
		}
	}

	ECYGPropertyType TypeOfTag(uint8 Tag)
	{
		switch (Tag)
		{
		case 1: return ECYGPropertyType::Bool;
		case 2: return ECYGPropertyType::Int;
		case 3: return ECYGPropertyType::Float;
		case 4: return ECYGPropertyType::Text;
		case 5: return ECYGPropertyType::Vector;
		case 6: return ECYGPropertyType::Rotation;
		default: return ECYGPropertyType::None;
		}
	}

	void WriteValue(const FCYGPropertyValue& Value, FCYGBitWriter& Target)
	{
		switch (Value.Type)
		{
		case ECYGPropertyType::Bool:		Target.WriteBool(Value.bBoolValue); break;
		case ECYGPropertyType::Int:			Target.WriteInt64(Value.IntValue); break;
		case ECYGPropertyType::Float:		Target.WriteSingle(Value.FloatValue); break;
		case ECYGPropertyType::Text:		Target.WriteString(Value.TextValue); break;
		case ECYGPropertyType::Vector:		Target.WriteVector3(Value.VectorValue); break;
		case ECYGPropertyType::Rotation:	Target.WriteQuaternion(Value.RotationValue); break;
		default: break;
		}
	}

	FCYGPropertyValue ReadValue(ECYGPropertyType Type, FCYGBitReader& Source)
	{
		FCYGPropertyValue Value;
		Value.Type = Type;

		switch (Type)
		{
		case ECYGPropertyType::Bool:		Value.bBoolValue = Source.ReadBool(); break;
		case ECYGPropertyType::Int:			Value.IntValue = Source.ReadInt64(); break;
		case ECYGPropertyType::Float:		Value.FloatValue = Source.ReadSingle(); break;
		case ECYGPropertyType::Text:		Value.TextValue = Source.ReadString(); break;
		case ECYGPropertyType::Vector:		Value.VectorValue = Source.ReadVector3(); break;
		case ECYGPropertyType::Rotation:	Value.RotationValue = Source.ReadQuaternion(); break;
		default: break;
		}

		return Value;
	}

	void Store(FCYGPropertyValue& Slot, ECYGPropertyType Type)
	{
		Slot.Type = Type;
		Slot.bBoolValue = false;
		Slot.IntValue = 0;
		Slot.FloatValue = 0.f;
		Slot.TextValue.Reset();
		Slot.VectorValue = FVector::ZeroVector;
		Slot.RotationValue = FQuat::Identity;
	}
}

void FCYGPropertyMap::SetBool(const FString& Key, bool Value)
{
	FCYGPropertyValue& Slot = Values.FindOrAdd(Key);

	if (Slot.Type == ECYGPropertyType::Bool && Slot.bBoolValue == Value)
	{
		return;
	}

	Store(Slot, ECYGPropertyType::Bool);
	Slot.bBoolValue = Value;
	DirtyKeys.Add(Key);
}

void FCYGPropertyMap::SetInt(const FString& Key, int64 Value)
{
	FCYGPropertyValue& Slot = Values.FindOrAdd(Key);

	if (Slot.Type == ECYGPropertyType::Int && Slot.IntValue == Value)
	{
		return;
	}

	Store(Slot, ECYGPropertyType::Int);
	Slot.IntValue = Value;
	DirtyKeys.Add(Key);
}

void FCYGPropertyMap::SetFloat(const FString& Key, float Value)
{
	FCYGPropertyValue& Slot = Values.FindOrAdd(Key);

	if (Slot.Type == ECYGPropertyType::Float && Slot.FloatValue == Value)
	{
		return;
	}

	Store(Slot, ECYGPropertyType::Float);
	Slot.FloatValue = Value;
	DirtyKeys.Add(Key);
}

void FCYGPropertyMap::SetText(const FString& Key, const FString& Value)
{
	FCYGPropertyValue& Slot = Values.FindOrAdd(Key);

	if (Slot.Type == ECYGPropertyType::Text && Slot.TextValue == Value)
	{
		return;
	}

	Store(Slot, ECYGPropertyType::Text);
	Slot.TextValue = Value;
	DirtyKeys.Add(Key);
}

void FCYGPropertyMap::SetVector(const FString& Key, const FVector& Value)
{
	FCYGPropertyValue& Slot = Values.FindOrAdd(Key);

	if (Slot.Type == ECYGPropertyType::Vector && Slot.VectorValue == Value)
	{
		return;
	}

	Store(Slot, ECYGPropertyType::Vector);
	Slot.VectorValue = Value;
	DirtyKeys.Add(Key);
}

void FCYGPropertyMap::SetRotation(const FString& Key, const FQuat& Value)
{
	FCYGPropertyValue& Slot = Values.FindOrAdd(Key);

	if (Slot.Type == ECYGPropertyType::Rotation && Slot.RotationValue.Equals(Value, KIND_SMALL_NUMBER))
	{
		return;
	}

	Store(Slot, ECYGPropertyType::Rotation);
	Slot.RotationValue = Value;
	DirtyKeys.Add(Key);
}

void FCYGPropertyMap::SetProperty(const FString& Key, FCYGPropertyValue Value)
{
	FCYGPropertyValue* Existing = Values.Find(Key);

	if (Existing != nullptr && *Existing == Value)
	{
		return;
	}

	Values.Add(Key, Value);
	DirtyKeys.Add(Key);
}

void FCYGPropertyMap::ApplyRemote(const FString& Key, const FCYGPropertyValue& Value)
{
	Values.Add(Key, Value);
}

bool FCYGPropertyMap::TryGet(const FString& Key, FCYGPropertyValue& OutValue) const
{
	const FCYGPropertyValue* Found = Values.Find(Key);

	if (Found == nullptr)
	{
		return false;
	}

	OutValue = *Found;
	return true;
}

bool FCYGPropertyMap::GetBool(const FString& Key) const
{
	FCYGPropertyValue Found;

	return TryGet(Key, Found) && Found.Type == ECYGPropertyType::Bool ? Found.bBoolValue : false;
}

int64 FCYGPropertyMap::GetInt(const FString& Key) const
{
	FCYGPropertyValue Found;

	if (!TryGet(Key, Found))
	{
		return 0;
	}

	if (Found.Type == ECYGPropertyType::Int)
	{
		return Found.IntValue;
	}

	return Found.Type == ECYGPropertyType::Float ? static_cast<int64>(Found.FloatValue) : 0;
}

float FCYGPropertyMap::GetFloat(const FString& Key) const
{
	FCYGPropertyValue Found;

	if (!TryGet(Key, Found))
	{
		return 0.f;
	}

	if (Found.Type == ECYGPropertyType::Float)
	{
		return Found.FloatValue;
	}

	return Found.Type == ECYGPropertyType::Int ? static_cast<float>(Found.IntValue) : 0.f;
}

FString FCYGPropertyMap::GetText(const FString& Key) const
{
	FCYGPropertyValue Found;

	return TryGet(Key, Found) && Found.Type == ECYGPropertyType::Text ? Found.TextValue : FString();
}

FVector FCYGPropertyMap::GetVector(const FString& Key) const
{
	FCYGPropertyValue Found;

	return TryGet(Key, Found) && Found.Type == ECYGPropertyType::Vector ? Found.VectorValue : FVector::ZeroVector;
}

FQuat FCYGPropertyMap::GetRotation(const FString& Key) const
{
	FCYGPropertyValue Found;

	return TryGet(Key, Found) && Found.Type == ECYGPropertyType::Rotation ? Found.RotationValue : FQuat::Identity;
}

bool FCYGPropertyMap::Contains(const FString& Key) const
{
	return Values.Contains(Key);
}

void FCYGPropertyMap::Remove(const FString& Key)
{
	Values.Remove(Key);
	DirtyKeys.Remove(Key);
}

void FCYGPropertyMap::Clear()
{
	Values.Reset();
	DirtyKeys.Reset();
}

void FCYGPropertyMap::SerializeChanges(FCYGBitWriter& Target)
{
	TArray<FString> Keys = DirtyKeys.Keys();
	const int32 Count = FMath::Min(Keys.Num(), 255);

	Target.WriteUInt8(static_cast<uint8>(Count));

	for (int32 Index = 0; Index < Count; ++Index)
	{
		const FCYGPropertyValue* Value = Values.Find(Keys[Index]);

		if (Value == nullptr)
		{
			continue;
		}

		Target.WriteString(Keys[Index]);
		Target.WriteUInt8(TagOf(Value->Type));
		WriteValue(*Value, Target);
	}

	DirtyKeys.Reset();
}

void FCYGPropertyMap::DeserializeChanges(FCYGBitReader& Source)
{
	DeserializeBlock(Source);
}

void FCYGPropertyMap::SerializeAll(FCYGBitWriter& Target) const
{
	Target.WriteUInt8(static_cast<uint8>(FMath::Min(Values.Num(), 255)));

	for (const TPair<FString, FCYGPropertyValue>& Entry : Values)
	{
		Target.WriteString(Entry.Key);
		Target.WriteUInt8(TagOf(Entry.Value.Type));
		WriteValue(Entry.Value, Target);
	}
}

void FCYGPropertyMap::DeserializeBlock(FCYGBitReader& Source)
{
	const uint8 Count = Source.ReadUInt8();

	for (uint8 Step = 0; Step < Count && !Source.HasError(); ++Step)
	{
		const FString Key = Source.ReadString();
		const ECYGPropertyType Type = TypeOfTag(Source.ReadUInt8());

		if (Type == ECYGPropertyType::None)
		{
			UE_LOG(LogCookYourGame, Warning, TEXT("Property \"%s\" arrived with a tag this build does not know."), *Key);
			return;
		}

		ApplyRemote(Key, ReadValue(Type, Source));
	}
}
