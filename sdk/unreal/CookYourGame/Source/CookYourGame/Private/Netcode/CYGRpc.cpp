// Copyright CookYourGame. All rights reserved.

#include "Netcode/CYGRpc.h"

#include "CookYourGame.h"
#include "Core/CYGClient.h"
#include "Gameplay/CYGView.h"
#include "Serialization/CYGBitPacker.h"
#include "UObject/UnrealType.h"

FOnCYGUnboundRpc FCYGRpcRouter::OnUnboundRpc;

namespace
{
	/** [viewId u32 big-endian][method hash u16][target u8] */
	constexpr int32 FrameHeaderSize = 7;

	struct FBinding
	{
		TWeakObjectPtr<UObject> Owner;
		UFunction* Function = nullptr;
	};

	/** View id to method hash to the function implementing it. */
	TMap<int32, TMap<uint16, FBinding>> Dispatch;

	/** Reflection identity of a mirrored math type, by its script struct name. */
	bool IsCarriedStruct(const FStructProperty* Struct, FName& OutKind)
	{
		OutKind = Struct->Struct != nullptr ? Struct->Struct->GetFName() : NAME_None;

		return OutKind == TEXT("Vector")
			|| OutKind == TEXT("Quat")
			|| OutKind == TEXT("Rotator")
			|| OutKind == TEXT("Vector2D");
	}

	bool WriteToProperty(FProperty* Property, void* Value, FCYGBitReader& Source)
	{
		if (CastField<FBoolProperty>(Property))
		{
			Property->SetIntPropertyValue(Value, Source.ReadBool() ? 1 : 0);
			return true;
		}

		if (CastField<FEnumProperty>(Property) || CastField<FByteProperty>(Property))
		{
			Property->SetIntPropertyValue(Value, Source.ReadInt32());
			return true;
		}

		if (FNumericProperty* Numeric = CastField<FNumericProperty>(Property))
		{
			if (Numeric->IsFloatingPoint())
			{
				Numeric->SetFloatingPointPropertyValue(Value, Source.ReadSingle());
			}
			else if (Numeric->IsUnsigned())
			{
				Numeric->SetUIntPropertyValue(Value, Source.ReadUInt32());
			}
			else if (Numeric->GetSize() == 8)
			{
				Numeric->SetIntPropertyValue(Value, Source.ReadInt64());
			}
			else
			{
				Numeric->SetIntPropertyValue(Value, Source.ReadInt32());
			}

			return true;
		}

		if (FNameProperty* Name = CastField<FNameProperty>(Property))
		{
			Name->SetPropertyValue(Value, FName(*Source.ReadString()));
			return true;
		}

		if (FStrProperty* Text = CastField<FStrProperty>(Property))
		{
			Text->SetPropertyValue(Value, Source.ReadString());
			return true;
		}

		if (FTextProperty* Localized = CastField<FTextProperty>(Property))
		{
			Localized->SetPropertyValue(Value, FText::FromString(Source.ReadString()));
			return true;
		}

		const FStructProperty* Struct = CastField<FStructProperty>(Property);
		FName Kind;

		if (Struct == nullptr || !IsCarriedStruct(Struct, Kind))
		{
			UE_LOG(LogCookYourGame, Warning, TEXT("Rpc argument %s is a type the wire does not carry."), *GetNameSafe(Property));
			return false;
		}

		if (Kind == TEXT("Vector"))
		{
			*static_cast<FVector*>(Value) = Source.ReadVector3();
		}
		else if (Kind == TEXT("Quat"))
		{
			*static_cast<FQuat*>(Value) = Source.ReadQuaternion();
		}
		else if (Kind == TEXT("Rotator"))
		{
			const FVector Axes = Source.ReadVector3Packed();
			*static_cast<FRotator*>(Value) = FRotator(Axes.X, Axes.Y, Axes.Z);
		}
		else
		{
			FVector2D* Flat = static_cast<FVector2D*>(Value);
			Flat->X = Source.ReadSingle();
			Flat->Y = Source.ReadSingle();
		}

		return !Source.HasError();
	}

	/** Decodes the frame into a parameter buffer and calls the bound function. */
	void InvokeBinding(const FBinding& Binding, const uint8* Arguments, int32 Size)
	{
		UObject* Owner = Binding.Owner.Get();
		UFunction* Function = Binding.Function;

		if (Owner == nullptr || Function == nullptr)
		{
			return;
		}

		uint8* Buffer = static_cast<uint8*>(FMemory::Malloc(Function->GetStructureSize()));
		Function->InitializeStruct(Buffer);

		FCYGBitReader Source(Arguments, Size);

		for (TFieldIterator<FProperty> Iterator(Function); Iterator && !Source.HasError(); ++Iterator)
		{
			FProperty* Property = *Iterator;

			// Only inputs travel; a method that hands something back is not an rpc.
			if ((Property->PropertyFlags & CPF_Parm) == 0 || (Property->PropertyFlags & CPF_ReturnParm) != 0)
			{
				continue;
			}

			if (!WriteToProperty(Property, Property->ContainerPtrToValuePtr<void>(Buffer), Source))
			{
				break;
			}
		}

		Owner->ProcessEvent(Function, Buffer);
		Function->DestroyStruct(Buffer);
		FMemory::Free(Buffer);
	}
}

uint16 FCYGRpcRouter::HashName(const FString& MethodName)
{
	// FNV-1a, 16-bit: offset basis 0x811C, prime 0x011D, masked after each multiply.
	uint32 Hash = 0x811Cu;

	for (int32 Index = 0; Index < MethodName.Len(); ++Index)
	{
		Hash ^= static_cast<uint32>(static_cast<uint8>(MethodName[Index]));
		Hash = (Hash * 0x011Du) & 0xFFFFu;
	}

	return static_cast<uint16>(Hash);
}

void FCYGRpcRouter::Bind(UObject* Owner, UCYGView* View, const TArray<FName>& MethodNames)
{
	if (!IsValid(Owner) || !IsValid(View) || View->ViewId <= 0)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("An rpc needs both an owner and a view with an id before it can bind."));
		return;
	}

	TMap<uint16, FBinding>& ByHash = Dispatch.FindOrAdd(View->ViewId);

	for (const FName& MethodName : MethodNames)
	{
		UFunction* Function = Owner->FindFunction(MethodName);

		if (Function == nullptr)
		{
			UE_LOG(LogCookYourGame, Error, TEXT("%s has no function \"%s\" to receive rpc calls."), *GetNameSafe(Owner), *MethodName.ToString());
			continue;
		}

		ByHash.Add(HashName(MethodName.ToString()), FBinding{ TWeakObjectPtr<UObject>(Owner), Function });
	}
}

void FCYGRpcRouter::Unbind(UCYGView* View)
{
	if (View != nullptr)
	{
		Dispatch.Remove(View->ViewId);
	}
}

void FCYGRpcRouter::Clear()
{
	Dispatch.Reset();
}

int32 FCYGRpcRouter::BoundViewCount()
{
	return Dispatch.Num();
}

bool FCYGRpcRouter::Send(UCYGView* View, const FString& MethodName, ECYGRPCTarget Target, const TArray<uint8>& Arguments, int32 TargetActorNumber)
{
	FCYGBitWriter Packed;
	Packed.WriteRaw(Arguments);

	return SendPacked(View, MethodName, Target, Packed, TargetActorNumber);
}

bool FCYGRpcRouter::SendPacked(UCYGView* View, const FString& MethodName, ECYGRPCTarget Target, FCYGBitWriter& Arguments, int32 TargetActorNumber)
{
	if (!IsValid(View) || View->ViewId <= 0)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("Rpc \"%s\" has no view to travel on."), *MethodName);
		return false;
	}

	UCYGClient* Client = View->GetClient();

	if (Client == nullptr || !Client->IsLinkConnected())
	{
		return false;
	}

	const uint16 Hash = HashName(MethodName);

	FCYGBitWriter Frame;
	Frame.WriteUInt32BE(static_cast<uint32>(View->ViewId));
	Frame.WriteUInt16BE(Hash);
	Frame.WriteUInt8(static_cast<uint8>(Target));

	if (Arguments.Num() > 0)
	{
		Frame.WriteRaw(Arguments.Data());
	}
	else
	{
		// A call with no arguments still needs one byte, or the frame would be
		// indistinguishable from a truncated one.
		Frame.WriteBool(true);
	}

	const TArray<uint8>& Payload = Frame.Data();

	if (!Client->SendRpcPayload(Payload))
	{
		return false;
	}

	if (ShouldEchoLocally(View, Target, TargetActorNumber))
	{
		const TMap<uint16, FBinding>* ByHash = Dispatch.Find(View->ViewId);

		if (ByHash != nullptr)
		{
			const FBinding* Binding = ByHash->Find(Hash);

			if (Binding != nullptr)
			{
				InvokeBinding(*Binding, Payload.GetData() + FrameHeaderSize, Payload.Num() - FrameHeaderSize);
			}
		}
	}

	return true;
}

bool FCYGRpcRouter::ShouldEchoLocally(UCYGView* View, ECYGRPCTarget Target, int32 TargetActorNumber)
{
	UCYGClient* Client = View != nullptr ? View->GetClient() : nullptr;

	switch (Target)
	{
	case ECYGRPCTarget::All:
		return true;

	case ECYGRPCTarget::Owner:
		return View != nullptr && View->IsMine();

	case ECYGRPCTarget::MasterClient:
		return Client != nullptr && Client->IsMasterClient();

	case ECYGRPCTarget::Target:
		return Client != nullptr && TargetActorNumber == Client->GetLocalActorNumber();

	default:
		return false;
	}
}

void FCYGRpcRouter::HandleFrame(int32 ViewId, uint16 MethodHash, uint8 Target, const uint8* Arguments, int32 Size, const FOnCYGUnboundRpc* Unbound)
{
	const TMap<uint16, FBinding>* ByHash = Dispatch.Find(ViewId);

	if (ByHash == nullptr)
	{
		return;
	}

	const FBinding* Binding = ByHash->Find(MethodHash);

	if (Binding == nullptr)
	{
		TArray<uint8> Copy;

		if (Arguments != nullptr && Size > 0)
		{
			Copy.Append(Arguments, Size);
		}

		if (Unbound != nullptr)
		{
			Unbound->Broadcast(MethodHash, Copy);
		}
		else
		{
			OnUnboundRpc.Broadcast(MethodHash, Copy);
		}

		return;
	}

	UCYGView* View = nullptr;

	// An owner-only call is dropped before it can touch game state.
	if (Target == static_cast<uint8>(ECYGRPCTarget::Owner) && (!FCYGViewTable::TryGet(ViewId, View) || View == nullptr || !View->IsMine()))
	{
		return;
	}

	InvokeBinding(*Binding, Arguments, Size);
}

void UCYGRpcComponent::OnRegister()
{
	Super::OnRegister();

	View = GetOwner() != nullptr ? GetOwner()->FindComponentByClass<UCYGView>() : nullptr;

	if (View != nullptr && RpcMethodNames.Num() > 0)
	{
		FCYGRpcRouter::Bind(GetOwner(), View, RpcMethodNames);
	}
}

void UCYGRpcComponent::OnUnregister()
{
	FCYGRpcRouter::Unbind(View);
	Super::OnUnregister();
}

void UCYGRpcComponent::SendRpc(const FString& MethodName, ECYGRPCTarget Target, const TArray<uint8>& Arguments)
{
	FCYGRpcRouter::Send(View, MethodName, Target, Arguments);
}

void UCYGRpcComponent::ServerRpc(const FString& MethodName, const TArray<uint8>& Arguments)
{
	FCYGRpcRouter::Send(View, MethodName, ECYGRPCTarget::MasterClient, Arguments);
}

void UCYGRpcComponent::ObserversRpc(const FString& MethodName, const TArray<uint8>& Arguments)
{
	FCYGRpcRouter::Send(View, MethodName, ECYGRPCTarget::Observers, Arguments);
}

void UCYGRpcComponent::TargetRpc(const FString& MethodName, int32 TargetActorNumber, const TArray<uint8>& Arguments)
{
	UCYGClient* Client = View != nullptr ? View->GetClient() : nullptr;
	FCYGPlayer Target;

	if (Client == nullptr || !Client->TryGetPlayer(TargetActorNumber, Target))
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("TargetRpc names actor %d, which is not in this room."), TargetActorNumber);
		return;
	}

	FCYGRpcRouter::Send(View, MethodName, ECYGRPCTarget::Target, Arguments, Target.ActorNumber);
}
