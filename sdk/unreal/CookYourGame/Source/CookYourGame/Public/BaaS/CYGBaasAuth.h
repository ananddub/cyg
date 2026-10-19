// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Templates/Function.h"

/** What the control plane says a player is, once a login took. */
struct FCYGBaasSession
{
	/** False when the account already existed and this call only signed into it. */
	bool bCreated = false;

	FString Token;
	FString RefreshToken;
	FString UserId;
	FString Username;
};

/** [tookTheAccount, session] The session is empty when the call failed. */
using FCYGBaasAuthResponse = TFunction<void(bool, const FCYGBaasSession&)>;

DECLARE_MULTICAST_DELEGATE_OneParam(FOnCYGAccountSignedIn, const FCYGBaasSession& /*Session*/);
DECLARE_MULTICAST_DELEGATE(FOnCYGAccountSignedOut);

/**
 * Player identity for the account endpoints.
 *
 * A login is the one call that hands back a credential, so it is also the only
 * one that stores it: every later request picks the bearer token up from the
 * http layer, and a game that signs out clears it everywhere at once.
 *
 * The session is written to the local user settings, the same plaintext store a
 * game's own options live in, so a restart does not have to log in again. A title
 * that must not keep a credential on disk sets bPersistSession false before its
 * first login and signs in every run.
 */
class COOKYOURGAME_API FCYGBaasAuth
{
public:
	/** Whether a session survives a restart. On by default, like a game's settings. */
	static bool bPersistSession;

	/** Guest login against a device id; empty asks for the stored one, made up on first use. */
	static void LoginDevice(const FCYGBaasAuthResponse& Done, const FString& Device = FString(), bool bCreate = true, const FString& Username = FString());

	static void LoginEmail(const FCYGBaasAuthResponse& Done, const FString& Email, const FString& Password, bool bCreate = false, const FString& Username = FString());

	static void LoginCustom(const FCYGBaasAuthResponse& Done, const FString& CustomId, bool bCreate = true, const FString& Username = FString());

	/** Forgets the session here and on disk. A game that quits to a menu calls this. */
	static void Logout();

	/**
	 * Adopts a session an earlier run stored, without asking the server.
	 * False when nothing was stored or the stored token is gone.
	 */
	static bool RestoreSession();

	static bool IsAuthenticated();

	static const FString& SessionToken();
	static const FString& RefreshToken();
	static const FString& UserId();
	static const FString& Username();

	/** Stable per-install id, generated and stored the first time anything asks for it. */
	static FString DeviceId();

	/** Raised after a login took, and after a stored session was restored. */
	static FOnCYGAccountSignedIn OnSignedIn;

	static FOnCYGAccountSignedOut OnSignedOut;

private:
	/** Turns one login reply into a session, stores it and reports the outcome. */
	static void HandleReply(const FString& Body, const FString& RequestedUsername, const FCYGBaasAuthResponse& Done);

	/** Reads the two claims this SDK cares about out of the token's payload. */
	static void ParseClaims(const FString& Jwt, FString& OutUserId, FString& OutUsername);

	static void StoreSession(const FCYGBaasSession& Session);
	static FString LoginPath(const TCHAR* Method, bool bCreate, const FString& Username);
};
