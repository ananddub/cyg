// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGEffects.h"

#include "CookYourGame.h"
#include "Core/CYGClient.h"
#include "Gameplay/CYGAssetRegistry.h"
#include "Gameplay/CYGCoordinateSpace.h"
#include "Kismet/GameplayStatics.h"
#include "NiagaraFunctionLibrary.h"
#include "NiagaraSystem.h"
#include "Serialization/CYGBitPacker.h"
#include "Sound/SoundBase.h"

namespace
{
	/** The room event name that carries effect messages. */
	const TCHAR* EffectEventName = TEXT("CYG_FX");
}

void UCYGEffects::AttachTo(UCYGClient* InClient)
{
	Detach();

	Client = InClient;

	if (InClient == nullptr)
	{
		return;
	}

	InClient->OnRoomEvent.AddDynamic(this, &UCYGEffects::HandleRoomEvent);
}

void UCYGEffects::Detach()
{
	if (UCYGClient* Previous = Client.Get())
	{
		Previous->OnRoomEvent.RemoveDynamic(this, &UCYGEffects::HandleRoomEvent);
	}

	Client = nullptr;
}

bool UCYGEffects::PlaySoundAtLocation(USoundBase* Sound, const FVector& Location, float VolumeMultiplier, float PitchMultiplier)
{
	if (Sound == nullptr)
	{
		return false;
	}

	uint16 AssetId = 0;

	if (!UCYGAssetRegistry::FindSoundId(Sound, AssetId))
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("%s is not in the networked asset registry, so the room cannot hear it."), *Sound->GetName());
		return false;
	}

	FCYGBitWriter Tail;
	Tail.WriteVector3Packed(FCYGCoordinateSpace::ToWire(Location));
	Tail.WriteSingle(VolumeMultiplier);
	Tail.WriteSingle(PitchMultiplier);

	if (!Broadcast(static_cast<uint8>(EKYGEffectKind::Sound), AssetId, Tail.Data()))
	{
		return false;
	}

	UGameplayStatics::PlaySoundAtLocation(this, Sound, Location, VolumeMultiplier, PitchMultiplier);
	return true;
}

bool UCYGEffects::PlayEffectAtLocation(UNiagaraSystem* System, const FVector& Location, FRotator Rotation, float Scale)
{
	if (System == nullptr)
	{
		return false;
	}

	uint16 AssetId = 0;

	if (!UCYGAssetRegistry::FindEffectId(System, AssetId))
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("%s is not in the networked asset registry, so the room cannot see it."), *System->GetName());
		return false;
	}

	FCYGBitWriter Tail;
	Tail.WriteVector3Packed(FCYGCoordinateSpace::ToWire(Location));
	Tail.WriteQuaternion(FCYGCoordinateSpace::ToWire(Rotation.Quaternion()));
	Tail.WriteSingle(Scale);

	if (!Broadcast(static_cast<uint8>(EKYGEffectKind::Particle), AssetId, Tail.Data()))
	{
		return false;
	}

	UNiagaraFunctionLibrary::SpawnSystemAtLocation(this, System, Location, Rotation, FVector(Scale, Scale, Scale));
	return true;
}

bool UCYGEffects::Broadcast(uint8 Kind, uint16 AssetId, const TArray<uint8>& Tail)
{
	if (Client == nullptr)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("The effect layer has no client to speak through."));
		return false;
	}

	FCYGBitWriter Body;
	Body.WriteUInt8(Kind);
	Body.WriteUInt16BE(AssetId);
	Body.WriteUInt32(static_cast<uint32>(Client->GetLocalActorNumber()));
	Body.WriteRaw(Tail);

	return Client->SendRoomEvent(EffectEventName, Body.Data());
}

void UCYGEffects::HandleRoomEvent(const FString& IncomingName, const TArray<uint8>& Body)
{
	if (IncomingName != EffectEventName || Body.Num() == 0)
	{
		return;
	}

	FCYGBitReader Reader(Body.GetData(), Body.Num());
	const uint8 Kind = Reader.ReadUInt8();
	const uint16 AssetId = Reader.ReadUInt16BE();
	const int32 Sender = static_cast<int32>(Reader.ReadUInt32());
	const FVector Location = FCYGCoordinateSpace::FromWire(Reader.ReadVector3Packed());

	if (Reader.HasError())
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("An effect message was shorter than it claimed."));
		return;
	}

	// The room sends the sender its own broadcast back. It has already played it.
	if (Client != nullptr && Sender == Client->GetLocalActorNumber())
	{
		return;
	}

	if (Kind == static_cast<uint8>(EKYGEffectKind::Sound))
	{
		USoundBase* Sound = UCYGAssetRegistry::FindSound(AssetId);
		const float Volume = Reader.ReadSingle();
		const float Pitch = Reader.ReadSingle();

		if (Sound == nullptr)
		{
			UE_LOG(LogCookYourGame, Warning, TEXT("A peer asked for sound id %d, which this build does not know."), AssetId);
			return;
		}

		UGameplayStatics::PlaySoundAtLocation(this, Sound, Location, Volume, Pitch);
		return;
	}

	if (Kind == static_cast<uint8>(EKYGEffectKind::Particle))
	{
		UNiagaraSystem* System = UCYGAssetRegistry::FindEffect(AssetId);
		const FRotator Rotation = FCYGCoordinateSpace::FromWire(Reader.ReadQuaternion()).Rotator();
		const float Scale = Reader.ReadSingle();

		if (System == nullptr)
		{
			UE_LOG(LogCookYourGame, Warning, TEXT("A peer asked for effect id %d, which this build does not know."), AssetId);
			return;
		}

		UNiagaraFunctionLibrary::SpawnSystemAtLocation(this, System, Location, Rotation, FVector(Scale, Scale, Scale));
		return;
	}

	UE_LOG(LogCookYourGame, Warning, TEXT("An effect message named kind %d, which this build does not know."), Kind);
}
