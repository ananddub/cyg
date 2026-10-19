// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGAssetRegistry.h"

#include "CookYourGame.h"
#include "NiagaraSystem.h"
#include "Sound/SoundBase.h"

namespace
{
	TMap<uint16, TWeakObjectPtr<USoundBase>> SoundsById;
	TMap<const USoundBase*, uint16> IdsBySound;

	TMap<uint16, TWeakObjectPtr<UNiagaraSystem>> EffectsById;
	TMap<const UNiagaraSystem*, uint16> IdsByEffect;
}

void UCYGAssetRegistry::PostLoad()
{
	Super::PostLoad();

	for (const FCYGNetworkedSound& Entry : Sounds)
	{
		if (Entry.Sound != nullptr)
		{
			RegisterSound(Entry.Sound.Get(), static_cast<uint16>(FMath::Clamp(Entry.NetworkId, 0, 65535)));
		}
	}

	for (const FCYGNetworkedEffect& Entry : Effects)
	{
		if (Entry.Effect != nullptr)
		{
			RegisterEffect(Entry.Effect.Get(), static_cast<uint16>(FMath::Clamp(Entry.NetworkId, 0, 65535)));
		}
	}
}

void UCYGAssetRegistry::Activate()
{
	Clear();
	PostLoad();
}

void UCYGAssetRegistry::RegisterSound(USoundBase* Sound, uint16 NetworkId)
{
	if (Sound == nullptr)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("Sound id %d names no sound."), NetworkId);
		return;
	}

	if (const uint16* Existing = IdsBySound.Find(Sound))
	{
		SoundsById.Remove(*Existing);
	}

	// The sound that held this id is now unreachable from the table, and leaving its
	// reverse entry behind would let a peer resolve the id to two different assets.
	if (const TWeakObjectPtr<USoundBase>* Taken = SoundsById.Find(NetworkId))
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("Id %d was held by %s and now goes to %s."), NetworkId, *GetNameSafe(Taken->Get()), *Sound->GetName());
		IdsBySound.Remove(Taken->Get());
	}

	IdsBySound.Add(Sound, NetworkId);
	SoundsById.Add(NetworkId, Sound);
}

void UCYGAssetRegistry::RegisterEffect(UNiagaraSystem* Effect, uint16 NetworkId)
{
	if (Effect == nullptr)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("Effect id %d names no system."), NetworkId);
		return;
	}

	if (const uint16* Existing = IdsByEffect.Find(Effect))
	{
		EffectsById.Remove(*Existing);
	}

	if (const TWeakObjectPtr<UNiagaraSystem>* Taken = EffectsById.Find(NetworkId))
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("Id %d was held by %s and now goes to %s."), NetworkId, *GetNameSafe(Taken->Get()), *Effect->GetName());
		IdsByEffect.Remove(Taken->Get());
	}

	IdsByEffect.Add(Effect, NetworkId);
	EffectsById.Add(NetworkId, Effect);
}

USoundBase* UCYGAssetRegistry::FindSound(uint16 NetworkId)
{
	const TWeakObjectPtr<USoundBase>* Found = SoundsById.Find(NetworkId);

	return Found != nullptr ? Found->Get() : nullptr;
}

UNiagaraSystem* UCYGAssetRegistry::FindEffect(uint16 NetworkId)
{
	const TWeakObjectPtr<UNiagaraSystem>* Found = EffectsById.Find(NetworkId);

	return Found != nullptr ? Found->Get() : nullptr;
}

bool UCYGAssetRegistry::FindSoundId(const USoundBase* Sound, uint16& OutNetworkId)
{
	const uint16* Found = IdsBySound.Find(Sound);

	if (Found == nullptr)
	{
		return false;
	}

	OutNetworkId = *Found;
	return true;
}

bool UCYGAssetRegistry::FindEffectId(const UNiagaraSystem* Effect, uint16& OutNetworkId)
{
	const uint16* Found = IdsByEffect.Find(Effect);

	if (Found == nullptr)
	{
		return false;
	}

	OutNetworkId = *Found;
	return true;
}

void UCYGAssetRegistry::Clear()
{
	SoundsById.Reset();
	IdsBySound.Reset();
	EffectsById.Reset();
	IdsByEffect.Reset();
}
