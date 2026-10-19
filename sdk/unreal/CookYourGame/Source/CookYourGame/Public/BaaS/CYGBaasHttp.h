// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Templates/Function.h"

/** A finished control-plane call: did it work, what did the server say, and its body. */
using FCYGBaasResponse = TFunction<void(bool /*bSuccess*/, int32 /*StatusCode*/, const FString& /*Body*/)>;

/** A call whose only answer is whether the server took it. */
using FCYGBaasBoolResponse = TFunction<void(bool /*bSuccess*/)>;

/**
 * Http for the account, storage, leaderboard and inventory endpoints.
 *
 * One place decides the endpoint, the timeout and the bearer token, so a game
 * that logs in once never handles a credential again: every call that needs one
 * picks up the token the auth layer published.
 *
 * A request only stays alive while something holds it, and the engine holds just
 * a weak reference, so this class keeps the strong one until the answer arrives.
 * Answers are delivered on the game thread, which is what lets that bookkeeping
 * be plain containers.
 */
class COOKYOURGAME_API FCYGBaasHttp
{
public:
	/** Overrides the configured endpoint while non-empty; the default is the room server's. */
	static FString BaseUrl;

	/** Seconds before a call is abandoned. */
	static int32 TimeoutSeconds;

	/** Published by the auth layer; sent as Authorization: Bearer on calls that ask for it. */
	static void SetSessionToken(const FString& Token);

	static const FString& GetSessionToken();

	/** Empty when neither an override nor a configured endpoint is usable. */
	static FString ResolveBaseUrl();

	static void Get(const FString& Path, const FCYGBaasResponse& Done, bool bRequireAuth = true);

	static void Post(const FString& Path, const FString& JsonBody, const FCYGBaasResponse& Done, bool bRequireAuth = true);

	static void Put(const FString& Path, const FString& JsonBody, const FCYGBaasResponse& Done, bool bRequireAuth = true);

	/** The http DELETE verb. */
	static void Drop(const FString& Path, const FCYGBaasResponse& Done, bool bRequireAuth = true);

	/** Abandons every outstanding call, for a shutdown that will not read the answers. */
	static void CancelAll();

private:
	static void Send(const TCHAR* Verb, const FString& Path, const FString& JsonBody, const FCYGBaasResponse& Done, bool bRequireAuth);
};
