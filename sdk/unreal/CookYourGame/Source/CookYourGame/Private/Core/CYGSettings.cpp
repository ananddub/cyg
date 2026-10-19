// Copyright CookYourGame. All rights reserved.

#include "Core/CYGSettings.h"

#include "CookYourGame.h"
#include "Containers/StringConv.h"

const TCHAR* UCYGSettings::DefaultRoomName = TEXT("cyg_room");
const TCHAR* UCYGSettings::DefaultMatchmakeMethod = TEXT("joinOrCreate");

namespace
{
	FString WithoutTrailingSlashes(const FString& Value)
	{
		FString Base = Value;
		Base.TrimEndInline();

		while (Base.EndsWith(TEXT("/")) && !Base.Equals(TEXT("/"), ESearchCase::CaseSensitive))
		{
			Base.LeftChopInline(1);
		}

		return Base;
	}

	FString SocketScheme(const FString& HttpScheme)
	{
		return HttpScheme == TEXT("https") ? FString(TEXT("wss://")) : FString(TEXT("ws://"));
	}
}

UCYGSettings::UCYGSettings()
	: ServerHttpUrl(TEXT("http://localhost:7777"))
	, ServerWsPath(TEXT("/ws"))
	, RoomName(DefaultRoomName)
	, MaxPlayers(8)
	, ProtocolVersion(1)
	, MatchmakeMethod(DefaultMatchmakeMethod)
	, GamePort(7777)
	, RequestTimeoutSeconds(10)
	, PingIntervalMilliseconds(5000)
{
}

bool UCYGSettings::TryHttpBase(FString& OutScheme, FString& OutHost, int32& OutPort) const
{
	OutScheme.Reset();
	OutHost.Reset();
	OutPort = -1;

	const FString Base = WithoutTrailingSlashes(ServerHttpUrl);
	const int32 SchemeEnd = Base.Find(TEXT("://"), ESearchCase::IgnoreCase);

	if (SchemeEnd == INDEX_NONE)
	{
		UE_LOG(LogCookYourGame, Error, TEXT("ServerHttpUrl \"%s\" has no scheme; expected http:// or https://."), *ServerHttpUrl);
		return false;
	}

	OutScheme = Base.Left(SchemeEnd).ToLower();
	FString Remainder = Base.Mid(SchemeEnd + 3);

	if (OutScheme != TEXT("http") && OutScheme != TEXT("https"))
	{
		UE_LOG(LogCookYourGame, Error, TEXT("ServerHttpUrl \"%s\" is neither http nor https."), *ServerHttpUrl);
		return false;
	}

	OutPort = OutScheme == TEXT("https") ? 443 : 80;

	int32 PathStart = INDEX_NONE;

	if (Remainder.FindChar(TEXT('/'), PathStart))
	{
		Remainder = Remainder.Left(PathStart);
	}

	OutHost = Remainder;

	int32 Colon = INDEX_NONE;

	if (OutHost.FindChar(TEXT(':'), Colon))
	{
		const FString PortText = OutHost.Mid(Colon + 1);
		OutHost = OutHost.Left(Colon);

		if (!PortText.IsEmpty())
		{
			OutPort = FCString::Atoi(*PortText);
		}
	}

	if (OutHost.IsEmpty())
	{
		UE_LOG(LogCookYourGame, Error, TEXT("ServerHttpUrl \"%s\" names no host."), *ServerHttpUrl);
		return false;
	}

	return true;
}

FString UCYGSettings::BuildSocketBaseUrl() const
{
	FString Scheme;
	FString Host;
	int32 Port = -1;

	if (!TryHttpBase(Scheme, Host, Port))
	{
		return FString();
	}

	FString Url = SocketScheme(Scheme);
	Url += Host;

	if (Port > 0 && Port != (Scheme == TEXT("https") ? 443 : 80))
	{
		Url += FString::Printf(TEXT(":%d"), Port);
	}

	const FString Path = UCYGSettings::LeadingPath(ServerWsPath);

	if (!Path.IsEmpty())
	{
		Url += TEXT("/");
		Url += Path;
	}

	return Url;
}

FString UCYGSettings::BuildMatchmakeUrl(const FString& Method, const FString& Room) const
{
	const FString Base = WithoutTrailingSlashes(ServerHttpUrl);

	if (Base.IsEmpty())
	{
		UE_LOG(LogCookYourGame, Error, TEXT("ServerHttpUrl is empty, so no seat can be reserved."));
		return FString();
	}

	FString Resolved = Method;

	if (Resolved.IsEmpty())
	{
		Resolved = MatchmakeMethod;
	}

	if (Resolved.IsEmpty())
	{
		Resolved = FString(DefaultMatchmakeMethod);
	}

	FString Url = Base;
	Url += TEXT("/api/matchmake/");
	Url += EscapeSegment(Resolved);
	Url += TEXT("/");
	Url += EscapeSegment(NormalizeRoomName(Room));

	return Url;
}

FString UCYGSettings::BuildGameSocketUrl(const FString& ProcessId, const FString& Room, const FString& SessionId, const FString& SeatToken, int32 Port) const
{
	FString Scheme;
	FString Host;
	int32 HttpPort = -1;

	if (!TryHttpBase(Scheme, Host, HttpPort))
	{
		return FString();
	}

	const int32 DefaultPort = Scheme == TEXT("https") ? 443 : 80;
	const int32 ActivePort = Port > 0 ? Port : (GamePort > 0 ? GamePort : HttpPort);

	FString Url = SocketScheme(Scheme);
	Url += Host;

	if (ActivePort > 0 && ActivePort != DefaultPort)
	{
		Url += FString::Printf(TEXT(":%d"), ActivePort);
	}

	const FString Path = UCYGSettings::LeadingPath(ServerWsPath);

	if (!Path.IsEmpty())
	{
		Url += TEXT("/");
		Url += Path;
	}

	Url += TEXT("/");
	Url += EscapeSegment(ProcessId.IsEmpty() ? FString(TEXT("cyg")) : ProcessId);

	Url += TEXT("/");
	Url += EscapeSegment(Room.IsEmpty() ? NormalizeRoomName(RoomName) : Room);

	FString Query;

	if (!SessionId.IsEmpty())
	{
		Query += TEXT("?sessionId=");
		Query += EscapeSegment(SessionId);
	}

	if (!SeatToken.IsEmpty())
	{
		Query += Query.IsEmpty() ? TEXT("?") : TEXT("&");
		Query += TEXT("token=");
		Query += EscapeSegment(SeatToken);
	}

	Url += Query;
	return Url;
}

FString UCYGSettings::NormalizeRoomName(const FString& Room)
{
	if (Room.IsEmpty())
	{
		return FString(DefaultRoomName);
	}

	TArray<TCHAR> Clean;
	Clean.Reserve(Room.Len() + 1);

	for (int32 Index = 0; Index < Room.Len() && Clean.Num() < 64; ++Index)
	{
		const TCHAR Character = Room[Index];
		const bool bAllowed =
			(Character >= TEXT('a') && Character <= TEXT('z'))
			|| (Character >= TEXT('A') && Character <= TEXT('Z'))
			|| (Character >= TEXT('0') && Character <= TEXT('9'))
			|| Character == TEXT('-')
			|| Character == TEXT('_');

		if (bAllowed)
		{
			Clean.Add(Character);
		}
	}

	if (Clean.IsEmpty())
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("Room name \"%s\" has no legal characters; using \"%s\" instead."), *Room, DefaultRoomName);
		return FString(DefaultRoomName);
	}

	Clean.Add(TEXT('\0'));
	return FString(Clean.GetData());
}

FString UCYGSettings::EscapeSegment(const FString& Value)
{
	static const TCHAR Digits[] = TEXT("0123456789ABCDEF");

	if (Value.IsEmpty())
	{
		return FString();
	}

	const FTCHARToUTF8 Utf8(*Value);
	FString Encoded;
	Encoded.Reserve(Utf8.Length() * 3);

	for (int32 Index = 0; Index < Utf8.Length(); ++Index)
	{
		const uint8 Byte = static_cast<uint8>(Utf8.Get()[Index]);
		const bool bSafe =
			(Byte >= 'a' && Byte <= 'z')
			|| (Byte >= 'A' && Byte <= 'Z')
			|| (Byte >= '0' && Byte <= '9')
			|| Byte == '-'
			|| Byte == '_'
			|| Byte == '.'
			|| Byte == '~';

		if (bSafe)
		{
			Encoded.AppendChar(static_cast<TCHAR>(Byte));
			continue;
		}

		Encoded.AppendChar(TEXT('%'));
		Encoded.AppendChar(Digits[Byte >> 4]);
		Encoded.AppendChar(Digits[Byte & 0x0F]);
	}

	return Encoded;
}

FString UCYGSettings::LeadingPath(const FString& Value)
{
	FString Path = Value.TrimStartAndEnd();

	while (Path.StartsWith(TEXT("/")))
	{
		Path.RightChopInline(1);
	}

	while (Path.EndsWith(TEXT("/")) && !Path.IsEmpty())
	{
		Path.LeftChopInline(1);
	}

	return Path;
}
