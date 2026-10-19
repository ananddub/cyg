// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "BaaS/CYGBaasAuth.h"
#include "Blueprint/AsyncActionBase.h"
#include "CYGBaasAsyncActions.generated.h"

/** A control-plane call the server answered, whatever the answer said. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FOnCYGBaasAnswered, int32, StatusCode, const FString&, Body);

/** A control-plane call that never got an answer, or got one that refused it. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FOnCYGBaasRefused, int32, StatusCode, const FString&, Reason);

/** The player an account call signed in, with the credential it came with. */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_ThreeParams(FOnCYGAccountSignedIn, FString, UserId, FString, Username, FString, Token);

DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnCYGAccountRefused, FString, Reason);

/**
 * A blueprint node for one account, storage, leaderboard or inventory call.
 *
 * The endpoints all answer with json and all behave the same way from a graph, so
 * one node covers them and the graph reads the fields it wants with the engine's
 * own json nodes. OnSuccess fires for any answer the server sent, including a 404
 * or a 500: the status code says what happened, and OnFailure is for the calls
 * that never reached a server at all.
 */
UCLASS()
class COOKYOURGAME_API UCYGAsyncBaasCall : public UBlueprintAsyncActionBase
{
	GENERATED_BODY()

public:
	/** Sends one request. Method is GET, POST, PUT or DELETE; Path starts with a slash. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Account",
		meta = (BlueprintInternalUseOnly = "true", DisplayName = "Account Call (Async)"))
	static UCYGAsyncBaasCall* Send(const FString& Method, const FString& Path, const FString& JsonBody = FString(), bool bRequireAuth = true);

	virtual void Activate() override;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Account")
	FOnCYGBaasAnswered OnSuccess;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Account")
	FOnCYGBaasRefused OnFailure;

private:
	void Begin(const FString& Verb, const FString& InPath, const FString& InBody, bool bInRequireAuth);

	/** Stops the node from being reached again and lets it be collected. */
	void Finish();

	FString Verb;
	FString Path;
	FString Body;

	bool bNeedsAuth = true;
	bool bFinished = false;
};

/**
 * Signs a player in and reports who they are.
 *
 * A login is the one account call whose answer is a credential, so it gets its own
 * node rather than leaving graphs to pick a token out of a json string. The token
 * arrives on a pin because a game that talks to its own server needs it; keep it
 * off screens and out of logs.
 */
UCLASS()
class COOKYOURGAME_API UCYGAsyncAccountLogin : public UBlueprintAsyncActionBase
{
	GENERATED_BODY()

public:
	/** Guest login against this install's stored device id. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Account",
		meta = (BlueprintInternalUseOnly = "true", DisplayName = "Login Device (Async)"))
	static UCYGAsyncAccountLogin* Device(bool bCreate = true, const FString& Username = FString());

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Account",
		meta = (BlueprintInternalUseOnly = "true", DisplayName = "Login Email (Async)"))
	static UCYGAsyncAccountLogin* Email(const FString& Address, const FString& Password, bool bCreate = false, const FString& Username = FString());

	/** Login against an id the game supplies, for an account the title already has. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Account",
		meta = (BlueprintInternalUseOnly = "true", DisplayName = "Login Custom Id (Async)"))
	static UCYGAsyncAccountLogin* Custom(const FString& CustomId, bool bCreate = true, const FString& Username = FString());

	virtual void Activate() override;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Account")
	FOnCYGAccountSignedIn OnSuccess;

	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Account")
	FOnCYGAccountRefused OnFailure;

private:
	/** Which of the three logins a node was made for; decides the endpoint. */
	enum class EMethod : uint8
	{
		Device,
		Email,
		Custom
	};

	void Begin(EMethod InMethod, const FString& InFirst, const FString& InSecond, bool InCreate, const FString& InUsername);

	void Finish();

	void HandleResult(bool bTook, const FCYGBaasSession& Session);

	EMethod Method = EMethod::Device;

	FString First;
	FString Second;
	FString Nickname;

	bool bCreate = true;
	bool bFinished = false;
};
