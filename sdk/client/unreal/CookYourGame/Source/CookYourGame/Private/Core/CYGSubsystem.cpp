// Copyright CookYourGame. All rights reserved.

#include "Core/CYGSubsystem.h"

#include "CookYourGame.h"
#include "Core/CYGClient.h"
#include "Engine/Engine.h"
#include "Engine/GameInstance.h"
#include "Engine/World.h"
#include "Gameplay/CYGActorManager.h"
#include "Gameplay/CYGActorPool.h"
#include "Gameplay/CYGEffects.h"
#include "Gameplay/CYGSceneManager.h"
#include "Gameplay/CYGView.h"
#include "HAL/PlatformTime.h"
#include "Netcode/CYGLagCompensation.h"
#include "Netcode/CYGRpc.h"
#include "Profiler/CYGNetworkProfiler.h"
#include "Resilience/CYGSessionCookie.h"
#include "Transport/CYGTransport.h"

void UCYGSubsystem::Initialize(FSubsystemCollectionBase& Collection)
{
	Super::Initialize(Collection);

	Client = NewObject<UCYGClient>(this);
	Client->Initialize();

	ActorManager = NewObject<UCYGActorManager>(this);
	ActorManager->AttachTo(Client, GetWorld());

	SceneManager = NewObject<UCYGSceneManager>(this);
	SceneManager->AttachTo(Client);

	Effects = NewObject<UCYGEffects>(this);
	Effects->AttachTo(Client);

	Client->OnConnected.AddDynamic(this, &UCYGSubsystem::HandleConnected);
	Client->OnDisconnected.AddDynamic(this, &UCYGSubsystem::HandleLinkLost);
	Client->OnJoinedRoom.AddDynamic(this, &UCYGSubsystem::HandleRoomTaken);

	// The resume loop rides this same pump, so it needs the client it has to reopen.
	FCYGSessionCookie::Attach(Client);

	AttachedWorld = GetWorld();

	TickerHandle = FTSTicker::GetCoreTicker().AddTicker(
		FTickerDelegate::CreateUObject(this, &UCYGSubsystem::PumpInbound));

	UE_LOG(LogCookYourGame, Display, TEXT("CookYourGame networking is ready."));
}

void UCYGSubsystem::Deinitialize()
{
	FTSTicker::GetCoreTicker().RemoveTicker(TickerHandle);

	if (Client != nullptr)
	{
		Client->OnConnected.RemoveDynamic(this, &UCYGSubsystem::HandleConnected);
		Client->OnDisconnected.RemoveDynamic(this, &UCYGSubsystem::HandleLinkLost);
		Client->OnJoinedRoom.RemoveDynamic(this, &UCYGSubsystem::HandleRoomTaken);

		// First the loop, so the shutdown close below cannot be read as a drop worth chasing.
		FCYGSessionCookie::Detach();

		Client->Disconnect();
	}

	if (ActorManager != nullptr)
	{
		ActorManager->Detach();
	}

	if (SceneManager != nullptr)
	{
		SceneManager->Detach();
	}

	if (Effects != nullptr)
	{
		Effects->Detach();
	}

	HandleLinkLost(TEXT("shut down"), 0);

	Client = nullptr;
	ActorManager = nullptr;
	SceneManager = nullptr;
	Effects = nullptr;

	Super::Deinitialize();
}

bool UCYGSubsystem::PumpInbound(float DeltaSeconds)
{
	if (Client == nullptr)
	{
		return true;
	}

	Client->Tick(DeltaSeconds);

	// Paced here rather than by the client, because a resume has to keep counting
	// while the link is down and the client's own tick is about the link.
	FCYGSessionCookie::Tick(DeltaSeconds);

	if (FCYGNetworkProfiler::bEnabled)
	{
		// The frame rate a room is pumped at is a number a traffic graph needs next to
		// its bytes, and this is the only loop that runs whether or not a link is up.
		FCYGNetworkProfiler::RecordTick(DeltaSeconds);
		FCYGNetworkProfiler::Pump(FPlatformTime::Seconds());
	}

	FollowWorldChange();

	if (Client->IsLocalReady())
	{
		const FCYGAuthority Authority{ Client->GetLocalActorNumber(), Client->IsMasterClient() };
		FCYGViewTable::Pump(Client, Authority);
	}

	return true;
}

void UCYGSubsystem::HandleConnected()
{
	// The level that is live when the room opens is the one spawn frames build in,
	// and it is not necessarily the level this subsystem was created for.
	FollowWorldChange();
}

void UCYGSubsystem::HandleLinkLost(const FString& Reason, int32 CloseCode)
{
	(void)Reason;

	FCYGViewTable::Clear();
	FCYGRpcRouter::Clear();

	// The rewind index names bodies that may be about to be torn down with the level,
	// and history from the room that just went away cannot validate anything in the next.
	FCYGLagCompensation::ClearBuffers();

	// Idle actors belong to a level and a room, and both are gone.
	FCYGActorPool::Clear();

	// Arms the bounded resume window when the seat it lost is still worth going back to.
	FCYGSessionCookie::NotifyDisconnected(CloseCode);
}

void UCYGSubsystem::HandleRoomTaken(const FString& RoomName)
{
	(void)RoomName;

	// The seat is live again, so there is nothing left for the resume loop to chase.
	FCYGSessionCookie::Clear();
}

void UCYGSubsystem::FollowWorldChange()
{
	UWorld* Live = GetWorld();

	if (Live == AttachedWorld.Get())
	{
		return;
	}

	AttachedWorld = Live;

	// Anything the old level held is gone with it, parked actors included.
	FCYGActorPool::Clear();

	if (ActorManager != nullptr)
	{
		ActorManager->AttachTo(Client, Live);
	}

	if (SceneManager != nullptr)
	{
		SceneManager->HandleMapLoaded(Live);
	}
}

void UCYGSubsystem::UseTransport(const TSharedPtr<FCYGTransport>& Transport)
{
	if (Client != nullptr)
	{
		Client->UseTransport(Transport);
	}
}

UCYGSubsystem* UCYGSubsystem::Get(const UObject* WorldContextObject)
{
	if (WorldContextObject == nullptr)
	{
		return nullptr;
	}

	const UWorld* World = WorldContextObject->GetWorld();
	UGameInstance* GameInstance = World != nullptr ? World->GetGameInstance() : nullptr;

	return GameInstance != nullptr ? GameInstance->GetSubsystem<UCYGSubsystem>() : nullptr;
}
