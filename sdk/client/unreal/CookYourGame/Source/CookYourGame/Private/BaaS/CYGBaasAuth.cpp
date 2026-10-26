// Copyright CookYourGame. All rights reserved.

#include "BaaS/CYGBaasAuth.h"

#include "BaaS/CYGBaasHttp.h"
#include "CYGBaasJson.h"
#include "CookYourGame.h"
#include "Containers/StringConv.h"
#include "Core/CYGSettings.h"
#include "Misc/Base64.h"
#include "Misc/ConfigCacheIni.h"

namespace
{
	/** Where a session lives between runs. A device id lives here too; it is not a secret. */
	const TCHAR* SessionSection = TEXT("CookYourGameSession");

	const TCHAR* KeyToken = TEXT("SessionToken");
	const TCHAR* KeyRefresh = TEXT("RefreshToken");
	const TCHAR* KeyUserId = TEXT("UserId");
	const TCHAR* KeyUsername = TEXT("Username");
	const TCHAR* KeyDevice = TEXT("DeviceId");

	FCYGBaasSession Current;

	FString ReadStored(const TCHAR* Key)
	{
		FString Value;

		if (GConfig != nullptr)
		{
			GConfig->GetString(SessionSection, Key, Value, GGameUserSettingsIni);
		}

		return Value;
	}

	void WriteStored(const TCHAR* Key, const FString& Value)
	{
		if (GConfig == nullptr)
		{
			return;
		}

		GConfig->SetString(SessionSection, Key, *Value, GGameUserSettingsIni);
		GConfig->Flush(false, GGameUserSettingsIni);
	}

	bool IsBearerUsable(const FString& Token)
	{
		return !Token.IsEmpty();
	}
}

bool FCYGBaasAuth::bPersistSession = true;
FOnCYGAccountSignedIn FCYGBaasAuth::OnSignedIn;
FOnCYGAccountSignedOut FCYGBaasAuth::OnSignedOut;

FString FCYGBaasAuth::DeviceId()
{
	static FString Cached;

	if (!Cached.IsEmpty())
	{
		return Cached;
	}

	Cached = ReadStored(KeyDevice);

	if (Cached.IsEmpty())
	{
		// Nothing stable identifies an install on every platform, so the first run
		// makes an id and keeps it. A title with a real device api can pass its own
		// value to LoginDevice instead.
		Cached = FGuid::NewGuid().ToString(EGuidFormats::Digits);
		WriteStored(KeyDevice, Cached);
	}

	return Cached;
}

FString FCYGBaasAuth::LoginPath(const TCHAR* Method, bool bCreate, const FString& Username)
{
	FString Path = FString::Printf(TEXT("/v2/account/authenticate/%s?create=%s"), Method, bCreate ? TEXT("true") : TEXT("false"));

	if (!Username.IsEmpty())
	{
		Path += FString::Printf(TEXT("&username=%s"), *UCYGSettings::EscapeSegment(Username));
	}

	return Path;
}

void FCYGBaasAuth::LoginDevice(const FCYGBaasAuthResponse& Done, const FString& Device, bool bCreate, const FString& Username)
{
	const FString Resolved = Device.IsEmpty() ? DeviceId() : Device;

	const TSharedRef<FJsonObject> Body = MakeShared<FJsonObject>();
	Body->SetStringField(TEXT("id"), Resolved);

	// A login is by definition anonymous: sending the previous account's bearer along
	// would ask the plane to prove a new identity with the credential of the old one.
	FCYGBaasHttp::Post(LoginPath(TEXT("device"), bCreate, Username), CYGBaasJson::Write(Body),
		[Username, Done](bool /*bSuccess*/, int32 /*Code*/, const FString& Reply)
		{
			FCYGBaasAuth::HandleReply(Reply, Username, Done);
		},
		/*bRequireAuth=*/false);
}

void FCYGBaasAuth::LoginEmail(const FCYGBaasAuthResponse& Done, const FString& Email, const FString& Password, bool bCreate, const FString& Username)
{
	const TSharedRef<FJsonObject> Body = MakeShared<FJsonObject>();
	Body->SetStringField(TEXT("email"), Email);
	Body->SetStringField(TEXT("password"), Password);

	FCYGBaasHttp::Post(LoginPath(TEXT("email"), bCreate, Username), CYGBaasJson::Write(Body),
		[Username, Done](bool /*bSuccess*/, int32 /*Code*/, const FString& Reply)
		{
			FCYGBaasAuth::HandleReply(Reply, Username, Done);
		},
		/*bRequireAuth=*/false);
}

void FCYGBaasAuth::LoginCustom(const FCYGBaasAuthResponse& Done, const FString& CustomId, bool bCreate, const FString& Username)
{
	if (CustomId.IsEmpty())
	{
		if (Done)
		{
			Done(false, FCYGBaasSession());
		}

		return;
	}

	const TSharedRef<FJsonObject> Body = MakeShared<FJsonObject>();
	Body->SetStringField(TEXT("id"), CustomId);

	FCYGBaasHttp::Post(LoginPath(TEXT("custom"), bCreate, Username), CYGBaasJson::Write(Body),
		[Username, Done](bool /*bSuccess*/, int32 /*Code*/, const FString& Reply)
		{
			FCYGBaasAuth::HandleReply(Reply, Username, Done);
		},
		/*bRequireAuth=*/false);
}

void FCYGBaasAuth::Logout()
{
	Current = FCYGBaasSession();
	FCYGBaasHttp::SetSessionToken(FString());

	if (bPersistSession)
	{
		WriteStored(KeyToken, FString());
		WriteStored(KeyRefresh, FString());
		WriteStored(KeyUserId, FString());
		WriteStored(KeyUsername, FString());
	}

	OnSignedOut.Broadcast();
}

bool FCYGBaasAuth::RestoreSession()
{
	const FString Token = ReadStored(KeyToken);

	if (!IsBearerUsable(Token))
	{
		return false;
	}

	Current = FCYGBaasSession();
	Current.Token = Token;
	Current.RefreshToken = ReadStored(KeyRefresh);
	Current.UserId = ReadStored(KeyUserId);
	Current.Username = ReadStored(KeyUsername);

	// A token restored without its claims is still the one the server issued.
	if (Current.UserId.IsEmpty())
	{
		ParseClaims(Current.Token, Current.UserId, Current.Username);
	}

	FCYGBaasHttp::SetSessionToken(Current.Token);
	OnSignedIn.Broadcast(Current);

	return true;
}

bool FCYGBaasAuth::IsAuthenticated()
{
	return IsBearerUsable(Current.Token);
}

const FString& FCYGBaasAuth::SessionToken()
{
	return Current.Token;
}

const FString& FCYGBaasAuth::RefreshToken()
{
	return Current.RefreshToken;
}

const FString& FCYGBaasAuth::UserId()
{
	return Current.UserId;
}

const FString& FCYGBaasAuth::Username()
{
	return Current.Username;
}

void FCYGBaasAuth::HandleReply(const FString& Body, const FString& RequestedUsername, const FCYGBaasAuthResponse& Done)
{
	const TSharedPtr<FJsonObject> Object = CYGBaasJson::Parse(Body);
	FCYGBaasSession Session;

	if (!Object.IsValid())
	{
		UE_LOG(LogCookYourGame, Error, TEXT("An account call answered with something that is not an object."));

		if (Done)
		{
			Done(false, Session);
		}

		return;
	}

	Session.Token = CYGBaasJson::ReadText(Object, TEXT("token"));
	Session.RefreshToken = CYGBaasJson::ReadText(Object, TEXT("refresh_token"));
	Session.bCreated = CYGBaasJson::ReadBool(Object, TEXT("created"));

	if (Session.Token.IsEmpty())
	{
		UE_LOG(LogCookYourGame, Error, TEXT("An account call gave no token: %s"), *CYGBaasJson::ReadError(Object));

		if (Done)
		{
			Done(false, Session);
		}

		return;
	}

	ParseClaims(Session.Token, Session.UserId, Session.Username);

	// The token is the authority on identity; a body that names the player covers a
	// token this build cannot decode, and the requested nickname covers that too.
	const TSharedPtr<FJsonObject> User = CYGBaasJson::ReadObject(Object, TEXT("user"));

	if (Session.UserId.IsEmpty())
	{
		Session.UserId = CYGBaasJson::ReadText(User, TEXT("id"));
	}

	if (Session.Username.IsEmpty())
	{
		Session.Username = CYGBaasJson::ReadText(User, TEXT("username"));
	}

	if (Session.Username.IsEmpty())
	{
		Session.Username = RequestedUsername;
	}

	Current = Session;
	FCYGBaasHttp::SetSessionToken(Session.Token);
	StoreSession(Session);

	OnSignedIn.Broadcast(Session);

	if (Done)
	{
		Done(true, Session);
	}
}

void FCYGBaasAuth::ParseClaims(const FString& Jwt, FString& OutUserId, FString& OutUsername)
{
	OutUserId.Reset();
	OutUsername.Reset();

	TArray<FString> Parts;
	Jwt.ParseIntoArray(Parts, TEXT("."), true);

	if (Parts.Num() < 2)
	{
		return;
	}

	// A jwt payload is base64url: the same alphabet with two swapped characters,
	// and its padding dropped.
	FString Payload = Parts[1].Replace(TEXT("-"), TEXT("+")).Replace(TEXT("_"), TEXT("/"));

	while (Payload.Len() % 4 != 0)
	{
		Payload.AppendChar(TEXT('='));
	}

	TArray<uint8> Bytes;

	if (!FBase64::Decode(Payload, Bytes))
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("A session token was not readable as base64, so its claims were skipped."));
		return;
	}

	const FUTF8ToTCHAR Converted(reinterpret_cast<const ANSICHAR*>(Bytes.GetData()), Bytes.Num());
	const TSharedPtr<FJsonObject> Claims = CYGBaasJson::Parse(FString(Converted.Get()));

	OutUserId = CYGBaasJson::ReadText(Claims, TEXT("uid"));
	OutUsername = CYGBaasJson::ReadText(Claims, TEXT("usn"));
}

void FCYGBaasAuth::StoreSession(const FCYGBaasSession& Session)
{
	// The device id outlives a session and is not a credential, so it is written
	// whether or not the title asked for its logins to survive.
	DeviceId();

	if (!bPersistSession)
	{
		return;
	}

	WriteStored(KeyToken, Session.Token);
	WriteStored(KeyRefresh, Session.RefreshToken);
	WriteStored(KeyUserId, Session.UserId);
	WriteStored(KeyUsername, Session.Username);
}
