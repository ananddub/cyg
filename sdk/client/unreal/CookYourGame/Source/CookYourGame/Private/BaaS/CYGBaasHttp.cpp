// Copyright CookYourGame. All rights reserved.

#include "BaaS/CYGBaasHttp.h"

#include "CookYourGame.h"
#include "Core/CYGSettings.h"
#include "HttpModule.h"
#include "Interfaces/IHttpRequest.h"
#include "Interfaces/IHttpResponse.h"

namespace
{
	/** Requests this layer is holding for the engine, by issue number. */
	TMap<int64, TSharedRef<IHttpRequest, ESPMode::ThreadSafe>> Outstanding;

	int64 NextRequestId = 0;
	FString CurrentToken;

	FString WithoutTrailingSlashes(const FString& Value)
	{
		FString Trimmed = Value;

		while (Trimmed.EndsWith(TEXT("/")))
		{
			Trimmed = Trimmed.Left(Trimmed.Len() - 1);
		}

		return Trimmed;
	}

	FString WithLeadingSlash(const FString& Value)
	{
		return Value.StartsWith(TEXT("/")) ? Value : FString(TEXT("/")) + Value;
	}
}

FString FCYGBaasHttp::BaseUrl;
int32 FCYGBaasHttp::TimeoutSeconds = 15;

void FCYGBaasHttp::SetSessionToken(const FString& Token)
{
	CurrentToken = Token;
}

const FString& FCYGBaasHttp::GetSessionToken()
{
	return CurrentToken;
}

FString FCYGBaasHttp::ResolveBaseUrl()
{
	if (!BaseUrl.IsEmpty())
	{
		return WithoutTrailingSlashes(BaseUrl);
	}

	const FString Configured = GetDefault<UCYGSettings>()->ServerHttpUrl;

	return Configured.IsEmpty() ? FString() : WithoutTrailingSlashes(Configured);
}

void FCYGBaasHttp::Get(const FString& Path, const FCYGBaasResponse& Done, bool bRequireAuth)
{
	Send(TEXT("GET"), Path, FString(), Done, bRequireAuth);
}

void FCYGBaasHttp::Post(const FString& Path, const FString& JsonBody, const FCYGBaasResponse& Done, bool bRequireAuth)
{
	Send(TEXT("POST"), Path, JsonBody, Done, bRequireAuth);
}

void FCYGBaasHttp::Put(const FString& Path, const FString& JsonBody, const FCYGBaasResponse& Done, bool bRequireAuth)
{
	Send(TEXT("PUT"), Path, JsonBody, Done, bRequireAuth);
}

void FCYGBaasHttp::Drop(const FString& Path, const FCYGBaasResponse& Done, bool bRequireAuth)
{
	Send(TEXT("DELETE"), Path, FString(), Done, bRequireAuth);
}

void FCYGBaasHttp::CancelAll()
{
	// Moving the table out first matters: cancelling a request reports a failed
	// completion, and that handler removes from this table.
	TMap<int64, TSharedRef<IHttpRequest, ESPMode::ThreadSafe>> Abandoned;
	Abandoned.swap(Outstanding);

	for (auto& Entry : Abandoned)
	{
		Entry.Value->CancelRequest();
	}
}

void FCYGBaasHttp::Send(const TCHAR* Verb, const FString& Path, const FString& JsonBody, const FCYGBaasResponse& Done, bool bRequireAuth)
{
	const FString Base = ResolveBaseUrl();

	if (Base.IsEmpty())
	{
		UE_LOG(LogCookYourGame, Error, TEXT("A %s call to %s has no endpoint. Set ServerHttpUrl in project settings, or CYGBaasHttp.BaseUrl."), Verb, *Path);

		if (Done)
		{
			Done(false, 0, FString());
		}

		return;
	}

	const TSharedRef<IHttpRequest, ESPMode::ThreadSafe> Request = FHttpModule::Get().CreateRequest();
	const int64 Id = ++NextRequestId;

	Request->SetVerb(Verb);
	Request->SetURL(Base + WithLeadingSlash(Path));
	Request->SetTimeout(static_cast<float>(FMath::Max(1, TimeoutSeconds)));

	if (!JsonBody.IsEmpty())
	{
		Request->SetContentAsString(JsonBody);
		Request->SetHeader(TEXT("Content-Type"), TEXT("application/json"));
	}

	if (bRequireAuth && !CurrentToken.IsEmpty())
	{
		Request->SetHeader(TEXT("Authorization"), FString(TEXT("Bearer ")) + CurrentToken);
	}

	Request->OnProcessRequestComplete().AddLambda(
		[Id, Done](const FHttpRequestPtr&, const FHttpResponsePtr& Response, bool bCompleted)
		{
			const int32 Code = Response.IsValid() ? Response->GetResponseCode() : 0;
			const FString Body = Response.IsValid() ? Response->GetContentAsString() : FString();

			Outstanding.Remove(Id);

			if (Done)
			{
				Done(bCompleted && Code >= 200 && Code < 300, Code, Body);
			}
		});

	Outstanding.Add(Id, Request);
	Request->ProcessRequest();
}
