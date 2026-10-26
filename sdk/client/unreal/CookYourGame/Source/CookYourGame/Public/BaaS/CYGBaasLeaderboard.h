// Copyright CookYourGame. All rights reserved.

#pragma once

#include "BaaS/CYGBaasHttp.h"
#include "CoreMinimal.h"
#include "Templates/Function.h"

class FJsonObject;

/** One place on a leaderboard, with the score that put it there. */
struct FCYGBaasLeaderboardRecord
{
	FString LeaderboardId;
	FString OwnerId;
	FString Username;
	FString Metadata;
	FString CreateTime;
	FString UpdateTime;

	int64 Score = 0;
	int64 Subscore = 0;
	int32 NumScore = 0;
	int64 Rank = 0;
	int64 MaxNumScore = 0;
};

/** One page of a leaderboard, plus the players of the caller that appear on it. */
struct FCYGBaasLeaderboardPage
{
	TArray<FCYGBaasLeaderboardRecord> Records;
	TArray<FCYGBaasLeaderboardRecord> OwnerRecords;

	/** Pass to the next GetScores to walk on; empty at the end of the board. */
	FString NextCursor;

	FString PrevCursor;
};

/** [succeeded, page] The page is empty when the call failed. */
using FCYGBaasLeaderboardResponse = TFunction<void(bool, const FCYGBaasLeaderboardPage&)>;

/**
 * Leaderboards and high scores.
 *
 * A game reports every finished run and the server decides what a submission
 * does to the caller's record, so the ranking rule stays where it can be
 * enforced. The subscore is the tie breaker, which is where a run's completion
 * time goes.
 */
class COOKYOURGAME_API FCYGBaasLeaderboard
{
public:
	static void SubmitScore(const FString& LeaderboardId, int64 Score, const FCYGBaasBoolResponse& Done, int64 Subscore = 0, const FString& MetadataJson = TEXT("{}"));

	static void GetScores(const FString& LeaderboardId, const FCYGBaasLeaderboardResponse& Done, int32 Limit = 10, const FString& Cursor = FString());

private:
	static FCYGBaasLeaderboardRecord ParseRecord(const TSharedPtr<FJsonObject>& Object);

	/** Every record of one array field of a reply, appended to the caller's list. */
	static void AppendRecords(const TSharedPtr<FJsonObject>& Object, const TCHAR* Field, TArray<FCYGBaasLeaderboardRecord>& OutRecords);

	/** Fills a page from the reply, leaving it empty when the body is not a board. */
	static void ParsePage(const FString& Body, FCYGBaasLeaderboardPage& OutPage);
};
