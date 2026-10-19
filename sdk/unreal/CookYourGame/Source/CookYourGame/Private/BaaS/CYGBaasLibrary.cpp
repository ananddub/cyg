// Copyright CookYourGame. All rights reserved.

#include "BaaS/CYGBaasLibrary.h"

#include "BaaS/CYGBaasAuth.h"
#include "BaaS/CYGBaasHttp.h"

bool UCYGBaasLibrary::IsAuthenticated()
{
	return FCYGBaasAuth::IsAuthenticated();
}

bool UCYGBaasLibrary::RestoreStoredSession()
{
	return FCYGBaasAuth::RestoreSession();
}

void UCYGBaasLibrary::SignOut()
{
	FCYGBaasAuth::Logout();
}

FString UCYGBaasLibrary::GetUserId()
{
	return FCYGBaasAuth::UserId();
}

FString UCYGBaasLibrary::GetUsername()
{
	return FCYGBaasAuth::Username();
}

FString UCYGBaasLibrary::GetSessionToken()
{
	return FCYGBaasAuth::SessionToken();
}

FString UCYGBaasLibrary::GetRefreshToken()
{
	return FCYGBaasAuth::RefreshToken();
}

FString UCYGBaasLibrary::GetDeviceId()
{
	return FCYGBaasAuth::DeviceId();
}

void UCYGBaasLibrary::SetBaseUrl(const FString& Url)
{
	FCYGBaasHttp::BaseUrl = Url;
}

void UCYGBaasLibrary::SetTimeoutSeconds(int32 Seconds)
{
	FCYGBaasHttp::TimeoutSeconds = FMath::Max(1, Seconds);
}

void UCYGBaasLibrary::SetPersistsSession(bool bPersist)
{
	FCYGBaasAuth::bPersistSession = bPersist;
}
