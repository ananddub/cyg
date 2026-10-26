// Copyright CookYourGame. All rights reserved.

#include "Resilience/CYGSessionCookie.h"

#include "CookYourGame.h"
#include "Core/CYGClient.h"
#include "Transport/CYGWireProtocol.h"

namespace
{
	/** First wait of a fresh window, before any doubling. */
	const float InitialBackoffSeconds = 0.5f;

	/**
	 * How long one attempt may go unanswered before the loop treats it as dead. A socket
	 * that fails to open at all reports nothing back, and without a deadline the window
	 * would freeze on the attempt that never answered.
	 */
	const float AttemptTimeoutSeconds = 8.f;

	TWeakObjectPtr<UCYGClient> Client;

	FString Room;
	FString Session;

	bool bHasCookie = false;
	bool bReconnecting = false;

	/** An attempt is open, so its answer is what advances or abandons the loop. */
	bool bAttemptInFlight = false;

	float AttemptElapsedSeconds = 0.f;

	int32 AttemptCount = 0;
	float ElapsedSeconds = 0.f;
	float NextAttemptDelay = 0.f;
	float CurrentBackoff = InitialBackoffSeconds;
}

float FCYGSessionCookie::WindowSeconds = 30.f;
float FCYGSessionCookie::MaxBackoffSeconds = 4.f;
int32 FCYGSessionCookie::MaxAttempts = 6;

void FCYGSessionCookie::Attach(UCYGClient* InClient)
{
	Client = InClient;
}

void FCYGSessionCookie::Detach()
{
	Client.Reset();
	Clear();
}

void FCYGSessionCookie::Capture(const FString& RoomName, const FString& SessionName)
{
	Room = RoomName;
	Session = SessionName;
	bHasCookie = true;
	ResetAttemptState();
}

void FCYGSessionCookie::Clear()
{
	ResetAttemptState();
	bHasCookie = false;

	Room.Reset();
	Session.Reset();
}

void FCYGSessionCookie::NotifyDisconnected(int32 CloseCode)
{
	if (!bHasCookie)
	{
		return;
	}

	if (IsSeatGone(CloseCode))
	{
		// The room or the plane said no, which is different from saying nothing: that
		// reservation is not coming back, and reusing a refused token is the one way a
		// resume loop becomes a denial of itself.
		UE_LOG(LogCookYourGame, Warning, TEXT("Seat resume not armed (%d). %s"), CloseCode, *Describe());
		Clear();
		return;
	}

	if (!bReconnecting)
	{
		// The first drop of a live session: open the window and wait out the first backoff.
		ResetAttemptState();
		bReconnecting = true;
		NextAttemptDelay = CurrentBackoff;

		UE_LOG(LogCookYourGame, Warning, TEXT("Link lost. %s"), *Describe());
		return;
	}

	// An attempt this loop fired came back dead: keep the window open and wait longer.
	bAttemptInFlight = false;
	ScheduleNextBackoff();
}

bool FCYGSessionCookie::TryResume()
{
	if (!ShouldAttempt())
	{
		return false;
	}

	UCYGClient* Target = Client.Get();

	if (Target == nullptr)
	{
		GiveUp(TEXT("the client that held the seat is gone."));
		return false;
	}

	++AttemptCount;
	bAttemptInFlight = true;
	AttemptElapsedSeconds = 0.f;

	if (!Target->ReconnectToSeat())
	{
		bAttemptInFlight = false;
		UE_LOG(LogCookYourGame, Warning, TEXT("Resume attempt %d could not rebuild the room url."), AttemptCount);
		return false;
	}

	UE_LOG(LogCookYourGame, Warning, TEXT("Attempting to resume the seat in room \"%s\" (attempt %d of %d)."), *Room, AttemptCount, MaxAttempts);
	return true;
}

void FCYGSessionCookie::Tick(float DeltaSeconds)
{
	if (!bReconnecting || DeltaSeconds <= 0.f)
	{
		return;
	}

	// The window keeps counting through an open attempt: a room that answers nothing
	// must still run the loop out, not park it on the attempt that never replied.
	ElapsedSeconds += DeltaSeconds;

	if (ElapsedSeconds >= WindowSeconds)
	{
		GiveUp(FString::Printf(TEXT("the resume window closed after %g seconds."), WindowSeconds));
		return;
	}

	if (bAttemptInFlight)
	{
		AttemptElapsedSeconds += DeltaSeconds;

		if (AttemptElapsedSeconds >= AttemptTimeoutSeconds)
		{
			// The link never reported either way, so release the attempt and wait longer.
			bAttemptInFlight = false;
			ScheduleNextBackoff();
		}

		return;
	}

	NextAttemptDelay -= DeltaSeconds;

	if (NextAttemptDelay > 0.f)
	{
		return;
	}

	if (!TryResume())
	{
		// A refused attempt still moves the schedule along, so the loop cannot spin.
		ScheduleNextBackoff();
	}
}

bool FCYGSessionCookie::HasCookie()
{
	return bHasCookie;
}

bool FCYGSessionCookie::IsReconnecting()
{
	return bReconnecting;
}

const FString& FCYGSessionCookie::RoomId()
{
	return Room;
}

const FString& FCYGSessionCookie::SessionId()
{
	return Session;
}

int32 FCYGSessionCookie::Attempts()
{
	return AttemptCount;
}

float FCYGSessionCookie::WindowElapsed()
{
	return ElapsedSeconds;
}

float FCYGSessionCookie::NextAttemptIn()
{
	return bReconnecting ? FMath::Max(0.f, NextAttemptDelay) : 0.f;
}

FString FCYGSessionCookie::Describe()
{
	if (!bHasCookie)
	{
		return FString(TEXT("[CYGSessionCookie empty]"));
	}

	// A seat is described by where it is, not by what proves it.
	return FString::Printf(TEXT("[CYGSessionCookie room=\"%s\" session=\"%s\" attempts=%d]"),
		Room.IsEmpty() ? TEXT("(none)") : *Room,
		Session.IsEmpty() ? TEXT("(none)") : *Session,
		AttemptCount);
}

bool FCYGSessionCookie::ShouldAttempt()
{
	UCYGClient* Target = Client.Get();

	return bHasCookie
		&& !bAttemptInFlight
		&& Target != nullptr
		&& !Target->IsLinkConnected()
		&& ElapsedSeconds < WindowSeconds
		&& AttemptCount < MaxAttempts;
}

bool FCYGSessionCookie::IsSeatGone(int32 CloseCode)
{
	if (FCYGWireProtocol::IsTerminalClose(CloseCode))
	{
		return true;
	}

	// A rejected handshake reports its code before the link closes, and the close that
	// follows often carries nothing, so the client's own last answer is the other half.
	// The client clears it every time a link opens, so it can only describe this seat.
	const UCYGClient* Target = Client.Get();

	return Target != nullptr && FCYGWireProtocol::IsTerminalClose(Target->GetLastErrorCloseCode());
}

void FCYGSessionCookie::ResetAttemptState()
{
	bReconnecting = false;
	bAttemptInFlight = false;
	AttemptElapsedSeconds = 0.f;
	AttemptCount = 0;
	ElapsedSeconds = 0.f;
	NextAttemptDelay = 0.f;
	CurrentBackoff = InitialBackoffSeconds;
}

void FCYGSessionCookie::ScheduleNextBackoff()
{
	CurrentBackoff = NextBackoff(CurrentBackoff);
	NextAttemptDelay = CurrentBackoff;

	if (AttemptCount >= MaxAttempts)
	{
		GiveUp(FString::Printf(TEXT("the ceiling of %d attempts was reached."), MaxAttempts));
	}
}

void FCYGSessionCookie::GiveUp(const FString& Why)
{
	UE_LOG(LogCookYourGame, Warning, TEXT("Seat resume abandoned: %s"), *Why);
	Clear();
}

float FCYGSessionCookie::NextBackoff(float Current)
{
	const float Doubled = Current * 2.f;

	return Doubled > MaxBackoffSeconds ? MaxBackoffSeconds : Doubled;
}
