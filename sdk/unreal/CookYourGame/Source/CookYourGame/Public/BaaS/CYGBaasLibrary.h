// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Kismet/BlueprintFunctionLibrary.h"
#include "CYGBaasLibrary.generated.h"

/**
 * Blueprint access to the account layer's state.
 *
 * The calls themselves are async nodes; these are the handful of instant answers
 * a graph needs, such as whether a session was restored at start-up and whose id a
 * storage read should name.
 */
UCLASS()
class COOKYOURGAME_API UCYGBaasLibrary : public UBlueprintFunctionLibrary
{
	GENERATED_BODY()

public:
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Account", DisplayName = "Is Signed In")
	static bool IsAuthenticated();

	/** Adopts a session an earlier run stored, without asking the server. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Account", DisplayName = "Restore Session")
	static bool RestoreStoredSession();

	/** Forgets the session here and on disk. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Account", DisplayName = "Sign Out")
	static void SignOut();

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Account", DisplayName = "Account User Id")
	static FString GetUserId();

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Account", DisplayName = "Account Username")
	static FString GetUsername();

	/** The bearer token, for a title that talks to its own server with it. */
	UFUNCTION(BlueprintPure, Category = "CookYourGame|Account", DisplayName = "Account Session Token")
	static FString GetSessionToken();

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Account", DisplayName = "Account Refresh Token")
	static FString GetRefreshToken();

	/** Id this install logs in as when a graph does not name one. */
	UFUNCTION(BlueprintPure, Category = "CookYourGame|Account", DisplayName = "Account Device Id")
	static FString GetDeviceId();

	/** Overrides the configured endpoint for every account call. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Account", DisplayName = "Set Account Base Url")
	static void SetBaseUrl(const FString& Url);

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Account", DisplayName = "Set Account Timeout")
	static void SetTimeoutSeconds(int32 Seconds);

	/** Whether a session survives a restart. Off stops a credential being written to disk. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Account", DisplayName = "Persist Account Session")
	static void SetPersistsSession(bool bPersist);
};
