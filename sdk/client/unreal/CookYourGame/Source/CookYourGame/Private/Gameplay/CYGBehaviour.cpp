// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGBehaviour.h"

#include "CookYourGame.h"
#include "Gameplay/CYGObservable.h"
#include "Gameplay/CYGView.h"
#include "Netcode/CYGRpc.h"
#include "Serialization/CYGBitPacker.h"

void UCYGBehaviour::OnRegister()
{
	Super::OnRegister();

	View = GetOwner() != nullptr ? GetOwner()->FindComponentByClass<UCYGView>() : nullptr;

	if (View == nullptr)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("%s on %s has no UCYGView; its state will not sync."), *GetName(), *GetNameSafe(GetOwner()));
	}
}

void UCYGBehaviour::OnUnregister()
{
	UnregisterAsObservable();
	Super::OnUnregister();
}

bool UCYGBehaviour::HasAuthority() const
{
	return View != nullptr && View->HasAuthority();
}

bool UCYGBehaviour::IsMine() const
{
	return View != nullptr && View->IsMine();
}

void UCYGBehaviour::RegisterSyncVars(TArray<TSharedPtr<FCYGSyncVarBase>>&& NewVariables)
{
	for (const TSharedPtr<FCYGSyncVarBase>& Variable : NewVariables)
	{
		if (!Variable.IsValid() || Variables.Contains(Variable))
		{
			continue;
		}

		if (Variables.Num() >= 255)
		{
			UE_LOG(LogCookYourGame, Warning, TEXT("%s on %s passed 255 sync vars; the rest are ignored."), *GetName(), *GetNameSafe(GetOwner()));
			break;
		}

		Variable->Index = static_cast<uint8>(Variables.Num());
		Variables.Add(Variable);
	}

	RegisterAsObservable();
}

void UCYGBehaviour::RegisterAsObservable()
{
	if (bObserved || View == nullptr)
	{
		return;
	}

	if (View->AddObservable(this))
	{
		bObserved = true;
	}
}

void UCYGBehaviour::UnregisterAsObservable()
{
	if (!bObserved)
	{
		return;
	}

	if (View != nullptr)
	{
		View->RemoveObservable(this);
	}

	bObserved = false;
}

bool UCYGBehaviour::HasCYGDelta() const
{
	for (const TSharedPtr<FCYGSyncVarBase>& Variable : Variables)
	{
		if (Variable.IsValid() && Variable->IsDirty())
		{
			return true;
		}
	}

	return false;
}

void UCYGBehaviour::SerializeCYG(FCYGBitWriter& Target)
{
	uint8 DirtyCount = 0;

	for (const TSharedPtr<FCYGSyncVarBase>& Variable : Variables)
	{
		if (Variable.IsValid() && Variable->IsDirty())
		{
			++DirtyCount;
		}
	}

	Target.WriteUInt8(DirtyCount);

	for (const TSharedPtr<FCYGSyncVarBase>& Variable : Variables)
	{
		if (!Variable.IsValid() || !Variable->IsDirty())
		{
			continue;
		}

		Target.WriteUInt8(Variable->Index);
		Variable->PackDirty(Target);
	}
}

void UCYGBehaviour::DeserializeCYG(FCYGBitReader& Source)
{
	const uint8 Count = Source.ReadUInt8();

	for (uint8 Step = 0; Step < Count; ++Step)
	{
		const uint8 Index = Source.ReadUInt8();
		const TSharedPtr<FCYGSyncVarBase> Variable = FindVarByIndex(Index);

		if (!Variable.IsValid())
		{
			// A value of unknown length cannot be skipped, so the rest of this
			// component's blob is unreadable.
			UE_LOG(LogCookYourGame, Error, TEXT("%s got a delta for sync var %d, which it never registered."), *GetName(), Index);
			return;
		}

		Variable->ApplyRemote(Source);
	}
}

TSharedPtr<FCYGSyncVarBase> UCYGBehaviour::FindVarByIndex(uint8 Index) const
{
	for (const TSharedPtr<FCYGSyncVarBase>& Variable : Variables)
	{
		if (Variable.IsValid() && Variable->Index == Index)
		{
			return Variable;
		}
	}

	return nullptr;
}

void UCYGBehaviour::ServerRpc(const FString& MethodName, const TArray<uint8>& Arguments)
{
	FCYGRpcRouter::Send(View, MethodName, ECYGRPCTarget::MasterClient, Arguments);
}

void UCYGBehaviour::ObserversRpc(const FString& MethodName, const TArray<uint8>& Arguments)
{
	FCYGRpcRouter::Send(View, MethodName, ECYGRPCTarget::Observers, Arguments);
}

void UCYGBehaviour::TargetRpc(const FString& MethodName, int32 TargetActorNumber, const TArray<uint8>& Arguments)
{
	FCYGRpcRouter::Send(View, MethodName, ECYGRPCTarget::Target, Arguments, TargetActorNumber);
}
