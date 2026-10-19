// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"

class UCYGClient;

/**
 * The seat a room gave, and a bounded loop that tries to take it back.
 *
 * A dropped link does not have to end a match: the reservation stands for a
 * while, and reconnecting with the same token puts the player back in the seat,
 * with the same actor number and the same views, instead of joining as somebody
 * new. This holds what a resume needs and paces the attempts, so a control plane
 * that is briefly unreachable is not hammered: half a second, then one, two and
 * four, inside a window that closes thirty seconds after the link went down. An
 * attempt that answers nothing is released after eight seconds, so the window can
 * always run out instead of parking on a link that never reported.
 *
 * Only what a room call cannot already read about itself is kept here: the name
 * of the room and the session. The reconnection token stays with the client that
 * was handed it, so this layer holds no credential to leak into a log line, and
 * Describe can be printed anywhere.
 */
class COOKYOURGAME_API FCYGSessionCookie
{
public:
	/** Seconds the resume window stays open, counted from the moment the link dropped. */
	static float WindowSeconds;

	/** Ceiling of the exponential backoff between attempts. */
	static float MaxBackoffSeconds;

	/** Hard cap on attempts, even while the window still has time left. */
	static int32 MaxAttempts;

	/** The client whose seat this is; the loop asks it to reconnect. */
	static void Attach(UCYGClient* InClient);

	static void Detach();

	/** Records the seat a reservation named, so a later drop can be resumed. */
	static void Capture(const FString& Room, const FString& Session);

	/** Drops the seat and stops the loop: an explicit leave, or a seat that came back. */
	static void Clear();

	/**
	 * Arms the loop for a link that died mid-room, or advances it after a failed try.
	 * A close code that means the seat was refused or the host is going away stops the
	 * loop instead: that reservation is not coming back.
	 */
	static void NotifyDisconnected(int32 CloseCode = 0);

	/** One immediate attempt, when a seat is held, the window is open and nothing is in flight. */
	static bool TryResume();

	/** Paces the loop. The subsystem feeds this the frame delta every frame. */
	static void Tick(float DeltaSeconds);

	static bool HasCookie();

	static bool IsReconnecting();

	static const FString& RoomId();

	static const FString& SessionId();

	/** Attempts made so far in this disconnect. */
	static int32 Attempts();

	/** Seconds of the window consumed. */
	static float WindowElapsed();

	/** Seconds until the next attempt, 0 while idle. */
	static float NextAttemptIn();

	/** One masked line about the seat, safe to print anywhere. */
	static FString Describe();

private:
	static bool ShouldAttempt();

	/** True when the room said this seat is finished, so no attempt can succeed. */
	static bool IsSeatGone(int32 CloseCode);

	static void ResetAttemptState();

	static void ScheduleNextBackoff();

	static void GiveUp(const FString& Why);

	/** The next wait: doubled, capped. */
	static float NextBackoff(float Current);
};
