// Copyright CookYourGame. All rights reserved.

#include "Async/CYGBaasAsyncActions.h"

#include "BaaS/CYGBaasHttp.h"
#include "CookYourGame.h"

UCYGAsyncBaasCall* UCYGAsyncBaasCall::Send(const FString& Method, const FString& Path, const FString& JsonBody, bool bRequireAuth)
{
	UCYGAsyncBaasCall* Node = NewObject<UCYGAsyncBaasCall>();
	Node->Begin(Method, Path, JsonBody, bRequireAuth);

	return Node;
}

void UCYGAsyncBaasCall::Begin(const FString& Verb, const FString& InPath, const FString& InBody, bool bInRequireAuth)
{
	this->Verb = Verb.IsEmpty() ? FString(TEXT("GET")) : Verb;
	Path = InPath;
	Body = InBody;
	bNeedsAuth = bInRequireAuth;

	// A node is only reachable through the wire it sits on, so it holds itself
	// until it has answered.
	AddToRoot();
}

void UCYGAsyncBaasCall::Activate()
{
	if (bFinished)
	{
		return;
	}

	bFinished = true;

	const TWeakObjectPtr<UCYGAsyncBaasCall> Weak(this);
	const FCYGBaasResponse Report = [Weak](bool bSuccess, int32 StatusCode, const FString& Reply)
	{
		UCYGAsyncBaasCall* Node = Weak.Get();

		if (Node == nullptr)
		{
			return;
		}

		if (bSuccess)
		{
			Node->OnSuccess.Broadcast(StatusCode, Reply);
		}
		else
		{
			Node->OnFailure.Broadcast(StatusCode, StatusCode == 0 ? FString(TEXT("the endpoint never answered")) : FString(TEXT("the server refused the call")));
		}

		Node->Finish();
	};

	const bool bPost = Verb.Equals(TEXT("POST"), ESearchCase::IgnoreCase);
	const bool bPut = Verb.Equals(TEXT("PUT"), ESearchCase::IgnoreCase);
	const bool bDelete = Verb.Equals(TEXT("DELETE"), ESearchCase::IgnoreCase);

	if (bPost)
	{
		FCYGBaasHttp::Post(Path, Body, Report, bNeedsAuth);
	}
	else if (bPut)
	{
		FCYGBaasHttp::Put(Path, Body, Report, bNeedsAuth);
	}
	else if (bDelete)
	{
		FCYGBaasHttp::Drop(Path, Report, bNeedsAuth);
	}
	else
	{
		FCYGBaasHttp::Get(Path, Report, bNeedsAuth);
	}
}

void UCYGAsyncBaasCall::Finish()
{
	RemoveFromRoot();
}

UCYGAsyncAccountLogin* UCYGAsyncAccountLogin::Device(bool bCreate, const FString& Username)
{
	UCYGAsyncAccountLogin* Node = NewObject<UCYGAsyncAccountLogin>();
	Node->Begin(EMethod::Device, FString(), FString(), bCreate, Username);

	return Node;
}

UCYGAsyncAccountLogin* UCYGAsyncAccountLogin::Email(const FString& Address, const FString& Password, bool bCreate, const FString& Username)
{
	UCYGAsyncAccountLogin* Node = NewObject<UCYGAsyncAccountLogin>();
	Node->Begin(EMethod::Email, Address, Password, bCreate, Username);

	return Node;
}

UCYGAsyncAccountLogin* UCYGAsyncAccountLogin::Custom(const FString& CustomId, bool bCreate, const FString& Username)
{
	UCYGAsyncAccountLogin* Node = NewObject<UCYGAsyncAccountLogin>();
	Node->Begin(EMethod::Custom, CustomId, FString(), bCreate, Username);

	return Node;
}

void UCYGAsyncAccountLogin::Begin(EMethod InMethod, const FString& InFirst, const FString& InSecond, bool InCreate, const FString& InUsername)
{
	Method = InMethod;
	First = InFirst;
	Second = InSecond;
	Nickname = InUsername;
	bCreate = InCreate;

	AddToRoot();
}

void UCYGAsyncAccountLogin::Activate()
{
	if (bFinished)
	{
		return;
	}

	bFinished = true;

	const TWeakObjectPtr<UCYGAsyncAccountLogin> Weak(this);
	const FCYGBaasAuthResponse Report = [Weak](bool bTook, const FCYGBaasSession& Session)
	{
		UCYGAsyncAccountLogin* Node = Weak.Get();

		if (Node != nullptr)
		{
			Node->HandleResult(bTook, Session);
		}
	};

	switch (Method)
	{
	case EMethod::Email:
		FCYGBaasAuth::LoginEmail(Report, First, Second, bCreate, Nickname);
		break;

	case EMethod::Custom:
		FCYGBaasAuth::LoginCustom(Report, First, bCreate, Nickname);
		break;

	default:
		FCYGBaasAuth::LoginDevice(Report, FString(), bCreate, Nickname);
		break;
	}
}

void UCYGAsyncAccountLogin::HandleResult(bool bTook, const FCYGBaasSession& Session)
{
	if (bTook)
	{
		OnSuccess.Broadcast(Session.UserId, Session.Username, Session.Token);
	}
	else
	{
		OnFailure.Broadcast(FString(TEXT("the account endpoint refused the login")));
	}

	Finish();
}

void UCYGAsyncAccountLogin::Finish()
{
	RemoveFromRoot();
}
