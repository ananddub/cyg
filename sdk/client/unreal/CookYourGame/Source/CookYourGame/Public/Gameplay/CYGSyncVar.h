// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Serialization/CYGBitPacker.h"

/**
 * Slot identity and dirty bookkeeping shared by every synced value.
 *
 * The index is the wire identity of a var inside its behaviour, so it is
 * assigned once, in registration order, and never renumbered.
 */
class COOKYOURGAME_API FCYGSyncVarBase
{
public:
	virtual ~FCYGSyncVarBase() = default;

	/** True when the value changed and enough time has passed since the last send. */
	virtual bool IsDirty() const = 0;

	/** Writes the current value and marks the var clean. */
	virtual void PackDirty(FCYGBitWriter& Target) = 0;

	/** Reads a value, applies it without a change hook and marks the var clean. */
	virtual void ApplyRemote(FCYGBitReader& Source) = 0;

	/** Wire index inside the owning behaviour. */
	uint8 Index = 0;

	/** Minimum seconds between syncs of this var; zero means every tick. */
	float SendIntervalSeconds = 0.f;
};

/** Wire codecs used by FCYGSyncVar; specialise for another value type. */
namespace CYGSyncCodecs
{
	template <typename Type> void Pack(FCYGBitWriter& Target, const Type& Value);
	template <typename Type> Type Unpack(FCYGBitReader& Source);

	template <> inline void Pack<int32>(FCYGBitWriter& Target, const int32& Value) { Target.WriteInt32(Value); }
	template <> inline int32 Unpack<int32>(FCYGBitReader& Source) { return Source.ReadInt32(); }

	template <> inline void Pack<uint32>(FCYGBitWriter& Target, const uint32& Value) { Target.WriteUInt32(Value); }
	template <> inline uint32 Unpack<uint32>(FCYGBitReader& Source) { return Source.ReadUInt32(); }

	template <> inline void Pack<int64>(FCYGBitWriter& Target, const int64& Value) { Target.WriteInt64(Value); }
	template <> inline int64 Unpack<int64>(FCYGBitReader& Source) { return Source.ReadInt64(); }

	template <> inline void Pack<uint64>(FCYGBitWriter& Target, const uint64& Value) { Target.WriteUInt64(Value); }
	template <> inline uint64 Unpack<uint64>(FCYGBitReader& Source) { return Source.ReadUInt64(); }

	template <> inline void Pack<bool>(FCYGBitWriter& Target, const bool& Value) { Target.WriteBool(Value); }
	template <> inline bool Unpack<bool>(FCYGBitReader& Source) { return Source.ReadBool(); }

	template <> inline void Pack<float>(FCYGBitWriter& Target, const float& Value) { Target.WriteSingle(Value); }
	template <> inline float Unpack<float>(FCYGBitReader& Source) { return Source.ReadSingle(); }

	template <> inline void Pack<double>(FCYGBitWriter& Target, const double& Value) { Target.WriteSingle(static_cast<float>(Value)); }
	template <> inline double Unpack<double>(FCYGBitReader& Source) { return static_cast<double>(Source.ReadSingle()); }

	template <> inline void Pack<FString>(FCYGBitWriter& Target, const FString& Value) { Target.WriteString(Value); }
	template <> inline FString Unpack<FString>(FCYGBitReader& Source) { return Source.ReadString(); }

	template <> inline void Pack<FName>(FCYGBitWriter& Target, const FName& Value) { Target.WriteString(Value.ToString()); }
	template <> inline FName Unpack<FName>(FCYGBitReader& Source) { return FName(*Source.ReadString()); }

	/** Twelve bytes: the wire is single precision even where the engine is not. */
	template <> inline void Pack<FVector>(FCYGBitWriter& Target, const FVector& Value)
	{
		Target.WriteVector3(Value);
	}
	template <> inline FVector Unpack<FVector>(FCYGBitReader& Source) { return Source.ReadVector3(); }

	/** Four bytes, smallest-three packing. Transform sync uses this by default. */
	template <> inline void Pack<FQuat>(FCYGBitWriter& Target, const FQuat& Value)
	{
		Target.WriteQuaternion(Value);
	}
	template <> inline FQuat Unpack<FQuat>(FCYGBitReader& Source) { return Source.ReadQuaternion(); }

	/** Two bytes per axis at one hundredth of a world unit. */
	template <> inline void Pack<FRotator>(FCYGBitWriter& Target, const FRotator& Value)
	{
		Target.WriteVector3Packed(FVector(Value.Pitch, Value.Yaw, Value.Roll));
	}
	template <> inline FRotator Unpack<FRotator>(FCYGBitReader& Source)
	{
		const FVector Axes = Source.ReadVector3Packed();
		return FRotator(Axes.X, Axes.Y, Axes.Z);
	}
}

/** A synced value: assignment marks it dirty, a remote apply fires the hook. */
template <typename Type>
class FCYGSyncVar final : public FCYGSyncVarBase
{
public:
	using FOnChanged = TFunction<void(const Type& /*Old*/, const Type& /*New*/)>;

	FCYGSyncVar() = default;
	explicit FCYGSyncVar(const Type& InValue) : Value(InValue) {}

	Type Value {};

	/** Reads the value. */
	const Type& Get() const { return Value; }

	/** Assigns a value, marking the var dirty when it actually changed. */
	void Set(const Type& NewValue)
	{
		if (Value == NewValue)
		{
			return;
		}

		Value = NewValue;
		bDirty = true;
	}

	/** Writes a value without dirtying it or firing anything: hydration only. */
	void SetSilent(const Type& NewValue)
	{
		Value = NewValue;
		bDirty = false;
	}

	void OnChanged(FOnChanged Hook) { ChangedHook = MoveTemp(Hook); }

	virtual bool IsDirty() const override
	{
		if (!bDirty)
		{
			return false;
		}

		if (SendIntervalSeconds > 0.f && FPlatformTime::Seconds() - LastSentSeconds < SendIntervalSeconds)
		{
			return false;
		}

		return true;
	}

	virtual void PackDirty(FCYGBitWriter& Target) override
	{
		CYGSyncCodecs::Pack<Type>(Target, Value);
		Clean();
	}

	virtual void ApplyRemote(FCYGBitReader& Source) override
	{
		const Type OldValue = Value;
		const Type NewValue = CYGSyncCodecs::Unpack<Type>(Source);

		Value = NewValue;
		Clean();

		if (ChangedHook && !(OldValue == NewValue))
		{
			ChangedHook(OldValue, NewValue);
		}
	}

private:
	void Clean()
	{
		bDirty = false;
		LastSentSeconds = FPlatformTime::Seconds();
	}

	bool bDirty = false;
	double LastSentSeconds = 0.0;
	FOnChanged ChangedHook;
};
