// Copyright CookYourGame. All rights reserved.

#include "BaaS/CYGBaasLeaderboard.h"

#include "CookYourGame.h"
#include "Core/CYGSettings.h"
#include "CYGBaasJson.h"

void FCYGBaasLeaderboard::SubmitScore(const FString& LeaderboardId, int64 Score, const FCYGBaasBoolResponse& Done, int64 Subscore, const FString& MetadataJson)
{
	const TSharedRef<FJsonObject> Body = MakeShared<FJsonObject>();
	Body->SetNumberField(TEXT("score"), static_cast<double>(Score));
	Body->SetNumberField(TEXT("subscore"), static_cast<double>(Subscore));
	Body->SetStringField(TEXT("metadata"), MetadataJson);

	const FString Path = FString::Printf(TEXT("/v2/leaderboard/%s"), *UCYGSettings::EscapeSegment(LeaderboardId));

	FCYGBaasHttp::Post(Path, CYGBaasJson::Write(Body),
		[Done, LeaderboardId](bool bSuccess, int32 StatusCode, const FString& Reply)
		{
			if (!bSuccess)
			{
				UE_LOG(LogCookYourGame, Error, TEXT("Submitting to %s failed (%d): %s"), *LeaderboardId, StatusCode, *CYGBaasJson::ReadError(CYGBaasJson::Parse(Reply)));
			}

			if (Done)
			{
				Done(bSuccess);
			}
		});
}

void FCYGBaasLeaderboard::GetScores(const FString& LeaderboardId, const FCYGBaasLeaderboardResponse& Done, int32 Limit, const FString& Cursor)
{
	FString Path = FString::Printf(TEXT("/v2/leaderboard/%s?limit=%d"), *UCYGSettings::EscapeSegment(LeaderboardId), FMath::Max(1, Limit));

	if (!Cursor.IsEmpty())
	{
		Path += FString::Printf(TEXT("&cursor=%s"), *UCYGSettings::EscapeSegment(Cursor));
	}

	FCYGBaasHttp::Get(Path,
		[Done, LeaderboardId](bool bSuccess, int32 StatusCode, const FString& Reply)
		{
			FCYGBaasLeaderboardPage Page;

			if (bSuccess)
			{
				FCYGBaasLeaderboard::ParsePage(Reply, Page);
			}
			else
			{
				UE_LOG(LogCookYourGame, Error, TEXT("Reading %s failed (%d): %s"), *LeaderboardId, StatusCode, *CYGBaasJson::ReadError(CYGBaasJson::Parse(Reply)));
			}

			if (Done)
			{
				Done(bSuccess, Page);
			}
		});
}

void FCYGBaasLeaderboard::AppendRecords(const TSharedPtr<FJsonObject>& Object, const TCHAR* Field, TArray<FCYGBaasLeaderboardRecord>& OutRecords)
{
	for (const TSharedPtr<FJsonValue>& Element : CYGBaasJson::ReadArray(Object, Field))
	{
		OutRecords.Add(ParseRecord(CYGBaasJson::ReadElement(Element)));
	}
}

void FCYGBaasLeaderboard::ParsePage(const FString& Body, FCYGBaasLeaderboardPage& OutPage)
{
	const TSharedPtr<FJsonObject> Object = CYGBaasJson::Parse(Body);

	if (!Object.IsValid())
	{
		return;
	}

	AppendRecords(Object, TEXT("records"), OutPage.Records);
	AppendRecords(Object, TEXT("owner_records"), OutPage.OwnerRecords);

	OutPage.NextCursor = CYGBaasJson::ReadText(Object, TEXT("next_cursor"));
	OutPage.PrevCursor = CYGBaasJson::ReadText(Object, TEXT("prev_cursor"));
}

FCYGBaasLeaderboardRecord FCYGBaasLeaderboard::ParseRecord(const TSharedPtr<FJsonObject>& Object)
{
	FCYGBaasLeaderboardRecord Record;

	if (!Object.IsValid())
	{
		return Record;
	}

	Record.LeaderboardId = CYGBaasJson::ReadText(Object, TEXT("leaderboard_id"));
	Record.OwnerId = CYGBaasJson::ReadText(Object, TEXT("owner_id"));
	Record.Username = CYGBaasJson::ReadText(Object, TEXT("username"));
	Record.Metadata = CYGBaasJson::ReadText(Object, TEXT("metadata"));
	Record.CreateTime = CYGBaasJson::ReadText(Object, TEXT("create_time"));
	Record.UpdateTime = CYGBaasJson::ReadText(Object, TEXT("update_time"));
	Record.Score = CYGBaasJson::ReadInt(Object, TEXT("score"));
	Record.Subscore = CYGBaasJson::ReadInt(Object, TEXT("subscore"));
	Record.NumScore = static_cast<int32>(CYGBaasJson::ReadInt(Object, TEXT("num_score")));
	Record.Rank = CYGBaasJson::ReadInt(Object, TEXT("rank"));
	Record.MaxNumScore = CYGBaasJson::ReadInt(Object, TEXT("max_num_score"));

	return Record;
}
