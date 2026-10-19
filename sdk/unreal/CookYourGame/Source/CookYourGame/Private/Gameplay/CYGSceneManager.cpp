// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGSceneManager.h"

#include "CookYourGame.h"
#include "Core/CYGClient.h"
#include "Engine/LevelStreaming.h"
#include "Engine/World.h"
#include "Kismet/GameplayStatics.h"
#include "Serialization/CYGBitPacker.h"

namespace
{
	/** The room event name that carries scene changes, shared with the Unity client. */
	const TCHAR* SceneEventName = TEXT("CYG_SCENE");
}

void UCYGSceneManager::AttachTo(UCYGClient* InClient)
{
	Detach();

	Client = InClient;

	if (InClient == nullptr)
	{
		return;
	}

	InClient->OnRoomEvent.AddDynamic(this, &UCYGSceneManager::HandleRoomEvent);
}

void UCYGSceneManager::Detach()
{
	if (UCYGClient* Previous = Client.Get())
	{
		Previous->OnRoomEvent.RemoveDynamic(this, &UCYGSceneManager::HandleRoomEvent);
	}

	Client = nullptr;
}

bool UCYGSceneManager::LoadNetworkedScene(const FString& MapName, ECYGSceneLoadMode Mode)
{
	if (MapName.IsEmpty())
	{
		UE_LOG(LogCookYourGame, Error, TEXT("LoadNetworkedScene needs a map name."));
		return false;
	}

	if (Client == nullptr)
	{
		UE_LOG(LogCookYourGame, Error, TEXT("LoadNetworkedScene has no client to tell about the map."));
		return false;
	}

	if (!Client->IsMasterClient())
	{
		// A peer that loads on its own would leave the room holding two maps.
		UE_LOG(LogCookYourGame, Error, TEXT("Only the master client may load a networked scene; %s is a plain peer."), *MapName);
		return false;
	}

	FCYGBitWriter Body;
	Body.WriteString(MapName);
	Body.WriteUInt8(static_cast<uint8>(Mode));

	if (!Client->SendRoomEvent(SceneEventName, Body.Data()))
	{
		return false;
	}

	StartLoad(MapName, Mode);
	return true;
}

void UCYGSceneManager::HandleRoomEvent(const FString& EventName, const TArray<uint8>& Body)
{
	if (EventName != SceneEventName || Body.Num() == 0)
	{
		return;
	}

	FCYGBitReader Reader(Body.GetData(), Body.Num());
	const FString MapName = Reader.ReadString();
	const ECYGSceneLoadMode Mode = Reader.ReadUInt8() == static_cast<uint8>(ECYGSceneLoadMode::Additive)
		? ECYGSceneLoadMode::Additive
		: ECYGSceneLoadMode::Single;

	StartLoad(MapName, Mode);
}

void UCYGSceneManager::StartLoad(const FString& MapName, ECYGSceneLoadMode Mode)
{
	if (MapName.IsEmpty())
	{
		return;
	}

	// The master client hears its own broadcast, so the echo of a travel already
	// running must not start a second one.
	if (bLoadInFlight && MapName == CurrentNetworkScene)
	{
		return;
	}

	UWorld* Target = GetWorld();

	if (Target == nullptr)
	{
		UE_LOG(LogCookYourGame, Error, TEXT("The scene manager has no world to load %s into."), *MapName);
		return;
	}

	if (Mode == ECYGSceneLoadMode::Additive)
	{
		ULevelStreaming* Streaming = UGameplayStatics::GetStreamingLevel(Target, FName(*MapName));

		if (Streaming != nullptr && Streaming->GetLoadedLevel() != nullptr)
		{
			CurrentNetworkScene = MapName;
			return;
		}

		// Blocking, because a peer that starts playing against a half-loaded map
		// desyncs worse than one that waits a frame.
		UGameplayStatics::LoadStreamLevel(Target, FName(*MapName), true, true, FLatentActionInfo());

		CurrentNetworkScene = MapName;
		OnNetworkSceneLoaded.Broadcast(MapName);
		return;
	}

	CurrentNetworkScene = MapName;
	bLoadInFlight = true;

	UGameplayStatics::OpenLevel(Target, FName(*MapName));
}

void UCYGSceneManager::HandleMapLoaded(UWorld* LoadedWorld)
{
	if (!bLoadInFlight || LoadedWorld == nullptr)
	{
		return;
	}

	bLoadInFlight = false;
	OnNetworkSceneLoaded.Broadcast(CurrentNetworkScene);
}
