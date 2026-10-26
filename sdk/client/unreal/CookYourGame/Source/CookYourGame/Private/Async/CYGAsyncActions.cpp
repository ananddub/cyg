// Copyright CookYourGame. All rights reserved.

#include "Async/CYGAsyncActions.h"

#include "Core/CYGClient.h"
#include "Core/CYGSettings.h"
#include "Core/CYGSubsystem.h"

UCYGAsyncJoinRoom* UCYGAsyncJoinRoom::JoinRoom(const UObject* WorldContextObject, const FString& Room,
	const FString& PlayerName, float TimeoutSeconds)
{
	UCYGAsyncJoinRoom* Node = NewObject<UCYGAsyncJoinRoom>();
	Node->Begin(WorldContextObject, Room, FString(), PlayerName, TimeoutSeconds);

	return Node;
}

UCYGAsyncJoinRoom* UCYGAsyncJoinRoom::Matchmake(const UObject* WorldContextObject, const FString& Room,
	const FString& Method, const FString& PlayerName, float TimeoutSeconds)
{
	UCYGAsyncJoinRoom* Node = NewObject<UCYGAsyncJoinRoom>();
	Node->Begin(WorldContextObject, Room, Method, PlayerName, TimeoutSeconds);

	return Node;
}

void UCYGAsyncJoinRoom::Begin(const UObject* WorldContextObject, const FString& InRoom, const FString& InMethod,
	const FString& InPlayerName, float TimeoutSeconds)
{
	Context = const_cast<UObject*>(WorldContextObject);
	Room = InRoom;
	Method = InMethod;
	PlayerName = InPlayerName;

	// A node handed to the engine is only reachable through the wire it sits on, so it
	// keeps itself alive until it has reported an outcome.
	AddToRoot();

	if (UWorld* World = Context != nullptr ? Context->GetWorld() : nullptr)
	{
		RegisterWithWorld(World);
	}

	if (TimeoutSeconds > 0.f)
	{
		DeadlineSeconds = FPlatformTime::Seconds() + static_cast<double>(TimeoutSeconds);
	}
	else
	{
		// Without an explicit deadline, twice the seat timeout, so a control plane that
		// never answers still releases the node.
		const UCYGSettings& Active = GetDefault<UCYGSettings>();
		DeadlineSeconds = FPlatformTime::Seconds() + FMath::Max(1, Active.RequestTimeoutSeconds) * 2.0;
	}

	TickerHandle = FTSTicker::GetCoreTicker().AddTicker(
		FTickerDelegate::CreateUObject(this, &UCYGAsyncJoinRoom::WatchTimeout), 0.25f);
}

void UCYGAsyncJoinRoom::Activate()
{
	UCYGSubsystem* Subsystem = UCYGSubsystem::Get(Context);
	UCYGClient* Target = Subsystem != nullptr ? Subsystem->GetClient() : nullptr;

	if (Target == nullptr)
	{
		OnFailure.Broadcast(0, TEXT("This world has no CookYourGame subsystem."));
		Finish();
		return;
	}

	if (Room.IsEmpty())
	{
		OnFailure.Broadcast(0, TEXT("Join needs a room name."));
		Finish();
		return;
	}

	Client = Target;

	if (Method.IsEmpty())
	{
		Target->ConnectToRoom(Room, PlayerName);
	}
	else
	{
		Target->ConnectToRoomAs(Room, Method, PlayerName);
	}

	Target->OnJoinedRoom.AddDynamic(this, &UCYGAsyncJoinRoom::HandleJoinedRoom);
	Target->OnError.AddDynamic(this, &UCYGAsyncJoinRoom::HandleServerError);
	Target->OnDisconnected.AddDynamic(this, &UCYGAsyncJoinRoom::HandleLinkLost);
}

bool UCYGAsyncJoinRoom::WatchTimeout(float DeltaSeconds)
{
	(void)DeltaSeconds;

	if (bFinished)
	{
		return false;
	}

	if (FPlatformTime::Seconds() >= DeadlineSeconds)
	{
		OnFailure.Broadcast(0, FString::Printf(TEXT("No answer from %s before the deadline."), *Room));
		Finish();

		return false;
	}

	return true;
}

void UCYGAsyncJoinRoom::HandleJoinedRoom(const FString& RoomName)
{
	if (bFinished)
	{
		return;
	}

	UCYGClient* Target = Client.Get();

	OnSuccess.Broadcast(RoomName.IsEmpty() ? Room : RoomName,
		Target != nullptr ? Target->GetLocalActorNumber() : 0,
		Target != nullptr ? Target->GetSessionId() : FString());

	Finish();
}

void UCYGAsyncJoinRoom::HandleServerError(int32 Code, const FString& Message)
{
	if (bFinished)
	{
		return;
	}

	OnFailure.Broadcast(Code, Message);
	Finish();
}

void UCYGAsyncJoinRoom::HandleLinkLost(const FString& Reason, int32 CloseCode)
{
	if (bFinished)
	{
		return;
	}

	OnFailure.Broadcast(CloseCode, Reason);
	Finish();
}

void UCYGAsyncJoinRoom::Finish()
{
	if (bFinished)
	{
		return;
	}

	bFinished = true;

	FTSTicker::GetCoreTicker().RemoveTicker(TickerHandle);

	if (UCYGClient* Target = Client.Get())
	{
		Target->OnJoinedRoom.RemoveDynamic(this, &UCYGAsyncJoinRoom::HandleJoinedRoom);
		Target->OnError.RemoveDynamic(this, &UCYGAsyncJoinRoom::HandleServerError);
		Target->OnDisconnected.RemoveDynamic(this, &UCYGAsyncJoinRoom::HandleLinkLost);
	}

	Client.Reset();

	RemoveFromRoot();
}
