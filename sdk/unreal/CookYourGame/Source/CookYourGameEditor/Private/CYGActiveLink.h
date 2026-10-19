// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"

class UCYGClient;

/**
 * The room client an editor panel should be reading.
 *
 * The details panel and the traffic tab both run in the editor, while the client
 * lives in a game instance that only exists during a play session. Rather than
 * ask the engine which world is playing, this walks the subsystems the object
 * registry knows about and prefers the one whose link is actually up, which also
 * makes the right choice when a PIE client and a server-in-editor are both alive.
 */
struct FCYGActiveLink
{
	/** Connected client if there is one, otherwise any live client, otherwise null. */
	static UCYGClient* Find();
};
