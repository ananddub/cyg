// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGAnimatorSync.h"

#include "Animation/AnimInstance.h"
#include "Components/SkeletalMeshComponent.h"
#include "CookYourGame.h"
#include "Serialization/CYGBitPacker.h"
#include "UObject/UnrealType.h"

namespace
{
	/** A number on the wire for a value that is not a float. */
	double ToNumber(const FCYGAnimatorParameter& Parameter, double Number)
	{
		return Parameter.Type == ECYGAnimatorParameterType::Float ? Number : FMath::RoundToDouble(Number);
	}
}

UCYGAnimatorSync::UCYGAnimatorSync()
{
	PrimaryComponentTick.bCanEverTick = true;
	PrimaryComponentTick.TickGroup = TG_PrePhysics;
}

void UCYGAnimatorSync::OnRegister()
{
	PrimaryComponentTick.bCanEverTick = true;

	Super::OnRegister();

	RebuildCarriedState();
	RegisterAsObservable();
}

void UCYGAnimatorSync::RebuildCarriedState()
{
	Carried.Reset();
	LastNumbers.Reset();
	DirtyMask = 0;
	bHasBaseline = false;

	for (int32 Index = 0; Index < Parameters.Num(); ++Index)
	{
		if (Parameters[Index].Type == ECYGAnimatorParameterType::Trigger)
		{
			continue;
		}

		if (Carried.Num() >= MaxCarriedParameters)
		{
			UE_LOG(LogCookYourGame, Warning, TEXT("%s carries more than %d animated values; the rest are not synced."),
				*GetName(), MaxCarriedParameters);

			break;
		}

		Carried.Add(Index);
		LastNumbers.Add(0.0);
	}
}

void UCYGAnimatorSync::OnSpawned()
{
	PendingTriggers.Reset();
	DirtyMask = 0;
	bHasBaseline = false;
}

UAnimInstance* UCYGAnimatorSync::ResolveAnimInstance()
{
	if (Mesh == nullptr && GetOwner() != nullptr)
	{
		Mesh = GetOwner()->FindComponentByClass<USkeletalMeshComponent>();
	}

	// A mesh builds its anim instance some frames after the component registers, so
	// this is asked again rather than once.
	AnimInstance = Mesh != nullptr ? Mesh->GetAnimInstance() : nullptr;

	return AnimInstance;
}

void UCYGAnimatorSync::TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction)
{
	Super::TickComponent(DeltaTime, TickType, ThisTickFunction);

	if (!HasAuthority())
	{
		return;
	}

	const double NowSeconds = FPlatformTime::Seconds();

	if (NowSeconds < SampleUntilSeconds)
	{
		return;
	}

	SampleUntilSeconds = NowSeconds + FMath::Max(0.001f, SampleIntervalSeconds);
	SampleAuthority();
}

void UCYGAnimatorSync::SampleAuthority()
{
	UAnimInstance* Instance = ResolveAnimInstance();

	if (Instance == nullptr || Carried.Num() == 0)
	{
		return;
	}

	if (!bHasBaseline)
	{
		for (int32 Slot = 0; Slot < Carried.Num(); ++Slot)
		{
			ReadValue(Slot, Instance, LastNumbers[Slot]);
		}

		bHasBaseline = true;

		return;
	}

	for (int32 Slot = 0; Slot < Carried.Num(); ++Slot)
	{
		if ((DirtyMask & (1ull << Slot)) != 0)
		{
			// Already waiting to go out; a value a setter queued is not polled over.
			continue;
		}

		double Current = 0.0;

		if (!ReadValue(Slot, Instance, Current))
		{
			continue;
		}

		const FCYGAnimatorParameter& Parameter = Parameters[Carried[Slot]];
		const double Delta = FMath::Abs(Current - LastNumbers[Slot]);
		const bool bMoved = Parameter.Type == ECYGAnimatorParameterType::Float
			? Delta > FMath::Abs(FloatDeadZone)
			: Delta > 0.0;

		if (bMoved)
		{
			LastNumbers[Slot] = Current;
			DirtyMask |= 1ull << Slot;
		}
	}
}

bool UCYGAnimatorSync::ReadValue(int32 Slot, UAnimInstance* Instance, double& OutNumber) const
{
	FProperty* Property = FindProperty(Slot, Instance);

	if (Property == nullptr || Instance == nullptr)
	{
		return false;
	}

	const void* Value = Property->ContainerPtrToValuePtr<void>(Instance);

	if (const FBoolProperty* Flag = CastField<FBoolProperty>(Property))
	{
		OutNumber = Flag->GetPropertyValue(Value) ? 1.0 : 0.0;

		return true;
	}

	if (const FNumericProperty* Number = CastField<FNumericProperty>(Property))
	{
		OutNumber = Number->IsFloatingPoint()
			? Number->GetFloatingPointPropertyValue(Value)
			: static_cast<double>(Number->GetIntPropertyValue(Value));

		return true;
	}

	return false;
}

bool UCYGAnimatorSync::WriteValue(int32 Slot, UAnimInstance* Instance, double Number) const
{
	FProperty* Property = FindProperty(Slot, Instance);

	if (Property == nullptr || Instance == nullptr)
	{
		return false;
	}

	void* Value = Property->ContainerPtrToValuePtr<void>(Instance);
	const FCYGAnimatorParameter& Parameter = Parameters[Carried[Slot]];

	if (FBoolProperty* Flag = CastField<FBoolProperty>(Property))
	{
		Flag->SetPropertyValue(Value, Number != 0.0);

		return true;
	}

	if (FNumericProperty* Numeric = CastField<FNumericProperty>(Property))
	{
		if (Parameter.Type == ECYGAnimatorParameterType::Float || Numeric->IsFloatingPoint())
		{
			Numeric->SetFloatingPointPropertyValue(Value, static_cast<float>(Number));
		}
		else
		{
			Numeric->SetIntPropertyValue(Value, static_cast<int64>(ToNumber(Parameter, Number)));
		}

		return true;
	}

	return false;
}

FProperty* UCYGAnimatorSync::FindProperty(int32 Slot, const UAnimInstance* Instance) const
{
	if (Instance == nullptr || !Carried.IsValidIndex(Slot) || !Parameters.IsValidIndex(Carried[Slot]))
	{
		return nullptr;
	}

	return Instance->GetClass()->FindPropertyByName(Parameters[Carried[Slot]].Name);
}

void UCYGAnimatorSync::InvokeTrigger(int32 ParameterIndex, UAnimInstance* Instance) const
{
	if (Instance == nullptr || !Parameters.IsValidIndex(ParameterIndex))
	{
		return;
	}

	UFunction* Handler = Instance->FindFunctionByName(Parameters[ParameterIndex].Name);

	if (Handler == nullptr)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("%s fired trigger \"%s\", which the anim graph does not implement."),
			*GetName(), *Parameters[ParameterIndex].Name.ToString());

		return;
	}

	Instance->ProcessEvent(Handler, nullptr);
}

int32 UCYGAnimatorSync::FindParameter(const FString& Name, ECYGAnimatorParameterType Type) const
{
	for (int32 Index = 0; Index < Parameters.Num(); ++Index)
	{
		if (Parameters[Index].Type == Type && Parameters[Index].Name.ToString().Equals(Name, ESearchCase::CaseSensitive))
		{
			return Index;
		}
	}

	return INDEX_NONE;
}

void UCYGAnimatorSync::FireTrigger(const FString& ParameterName)
{
	const int32 Index = FindParameter(ParameterName, ECYGAnimatorParameterType::Trigger);

	if (Index == INDEX_NONE)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("%s was asked to fire \"%s\", which it does not carry as a trigger."), *GetName(), *ParameterName);

		return;
	}

	UAnimInstance* Instance = ResolveAnimInstance();
	InvokeTrigger(Index, Instance);

	if (!HasAuthority())
	{
		return;
	}

	if (PendingTriggers.Num() >= MaxQueuedTriggers)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("%s dropped trigger \"%s\": %d already waiting."),
			*GetName(), *ParameterName, MaxQueuedTriggers);

		return;
	}

	PendingTriggers.Add(Index);
}

void UCYGAnimatorSync::SetFloatParameter(const FString& ParameterName, float Value)
{
	WriteParameter(ParameterName, ECYGAnimatorParameterType::Float, Value);
}

void UCYGAnimatorSync::SetBoolParameter(const FString& ParameterName, bool bValue)
{
	WriteParameter(ParameterName, ECYGAnimatorParameterType::Bool, bValue ? 1.0 : 0.0);
}

void UCYGAnimatorSync::SetIntegerParameter(const FString& ParameterName, int32 Value)
{
	WriteParameter(ParameterName, ECYGAnimatorParameterType::Integer, Value);
}

void UCYGAnimatorSync::WriteParameter(const FString& ParameterName, ECYGAnimatorParameterType Type, double Number)
{
	const int32 Index = FindParameter(ParameterName, Type);

	if (Index == INDEX_NONE)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("%s was handed \"%s\", which it does not carry as a %d."),
			*GetName(), *ParameterName, static_cast<int32>(Type));

		return;
	}

	const int32* Slot = Carried.Find(Index);
	UAnimInstance* Instance = AnimInstance != nullptr ? AnimInstance : ResolveAnimInstance();

	if (Slot == nullptr || Instance == nullptr)
	{
		return;
	}

	// Only the anim instance is written here. The authority's next poll notices the
	// change and sends it, which is the one path that keeps every peer in step.
	WriteValue(*Slot, Instance, Number);
}

bool UCYGAnimatorSync::HasCYGDelta() const
{
	return DirtyMask != 0 || PendingTriggers.Num() > 0 || UCYGBehaviour::HasCYGDelta();
}

void UCYGAnimatorSync::SerializeCYG(FCYGBitWriter& Sink)
{
	UCYGBehaviour::SerializeCYG(Sink);

	uint8 Flags = 0;

	if (DirtyMask != 0)
	{
		Flags |= ValuesFlag;
	}

	if (PendingTriggers.Num() > 0)
	{
		Flags |= TriggersFlag;
	}

	// Written even when clear, so the reader's layout matches a sync-var-only delta.
	Sink.WriteUInt8(Flags);

	if (Flags == 0)
	{
		return;
	}

	if ((Flags & ValuesFlag) != 0)
	{
		uint8 Count = 0;

		for (int32 Slot = 0; Slot < Carried.Num(); ++Slot)
		{
			if ((DirtyMask & (1ull << Slot)) != 0)
			{
				++Count;
			}
		}

		Sink.WriteUInt8(Count);

		for (int32 Slot = 0; Slot < Carried.Num(); ++Slot)
		{
			if ((DirtyMask & (1ull << Slot)) == 0)
			{
				continue;
			}

			const FCYGAnimatorParameter& Parameter = Parameters[Carried[Slot]];
			const double Number = ToNumber(Parameter, LastNumbers[Slot]);

			// The index of the authored list travels, not the slot, so both sides read
			// their own copy of the same declaration.
			Sink.WriteUInt8(static_cast<uint8>(Carried[Slot]));

			switch (Parameter.Type)
			{
			case ECYGAnimatorParameterType::Integer:
				Sink.WriteInt32(static_cast<int32>(Number));
				break;

			case ECYGAnimatorParameterType::Float:
				Sink.WriteSingle(static_cast<float>(Number));
				break;

			default:
				Sink.WriteBool(Number != 0.0);
				break;
			}

			DirtyMask &= ~(1ull << Slot);
		}
	}

	if ((Flags & TriggersFlag) != 0)
	{
		Sink.WriteUInt8(static_cast<uint8>(PendingTriggers.Num()));

		for (const int32 Index : PendingTriggers)
		{
			Sink.WriteUInt8(static_cast<uint8>(Index));
		}

		PendingTriggers.Reset();
	}
}

void UCYGAnimatorSync::DeserializeCYG(FCYGBitReader& Source)
{
	UCYGBehaviour::DeserializeCYG(Source);

	const uint8 Flags = Source.ReadUInt8();
	const bool bApply = !HasAuthority();

	if ((Flags & ValuesFlag) != 0)
	{
		const uint8 Count = Source.ReadUInt8();

		for (uint8 Step = 0; Step < Count; ++Step)
		{
			const uint8 Index = Source.ReadUInt8();

			if (!Parameters.IsValidIndex(Index) || Source.HasError())
			{
				// A value whose length is unknown cannot be stepped over, so the rest of
				// this blob is unreadable.
				UE_LOG(LogCookYourGame, Error, TEXT("%s got an animated value for slot %d, which it never declared."), *GetName(), Index);

				return;
			}

			const FCYGAnimatorParameter& Parameter = Parameters[Index];
			double Number = 0.0;

			switch (Parameter.Type)
			{
			case ECYGAnimatorParameterType::Integer:
				Number = Source.ReadInt32();
				break;

			case ECYGAnimatorParameterType::Float:
				Number = Source.ReadSingle();
				break;

			case ECYGAnimatorParameterType::Bool:
				Number = Source.ReadBool() ? 1.0 : 0.0;
				break;

			default:
				// A trigger in the value block means the two sides declared different lists.
				UE_LOG(LogCookYourGame, Error, TEXT("%s got a value for trigger \"%s\"; the parameter lists do not match."),
					*GetName(), *Parameter.Name.ToString());

				return;
			}

			if (!bApply)
			{
				continue;
			}

			UAnimInstance* Instance = AnimInstance != nullptr ? AnimInstance : ResolveAnimInstance();
			const int32* Slot = Carried.Find(Index);

			if (Slot != nullptr && Instance != nullptr)
			{
				WriteValue(*Slot, Instance, Number);
			}
		}
	}

	if ((Flags & TriggersFlag) != 0)
	{
		const uint8 Count = Source.ReadUInt8();

		UAnimInstance* Instance = bApply ? (AnimInstance != nullptr ? AnimInstance : ResolveAnimInstance()) : nullptr;

		for (uint8 Step = 0; Step < Count; ++Step)
		{
			const uint8 Index = Source.ReadUInt8();

			if (!Parameters.IsValidIndex(Index))
			{
				UE_LOG(LogCookYourGame, Error, TEXT("%s got a trigger for slot %d, which it never declared."), *GetName(), Index);

				return;
			}

			if (bApply)
			{
				InvokeTrigger(Index, Instance);
			}
		}
	}
}
