// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Templates/Function.h"

class FJsonObject;

/** One message from a `/v2/channel` stream. */
struct FCYGBaasChatMessage
{
	FString ChannelId;
	FString MessageId;
	int32 Code = 0;
	FString SenderId;
	FString Username;

	/** The server's JSON text for the content object, e.g. {"text":"gg"}. */
	FString Content;

	FString CreateTime;
	FString UpdateTime;
	bool bPersistent = false;
	FString RoomName;
	FString GroupId;
	FString UserIdOne;
	FString UserIdTwo;

	/** The `text` member when Content is a text object, else the raw content. */
	FString GetText() const;
};

/** One page of stored history and the cursors that walk it. */
struct FCYGBaasChatPage
{
	TArray<FCYGBaasChatMessage> Messages;
	FString NextCursor;
	FString PrevCursor;
	FString CacheableCursor;
};

using FCYGBaasChatPageResponse = TFunction<void(bool, const FCYGBaasChatPage&)>;
using FCYGBaasChatMessageResponse = TFunction<void(bool, const FCYGBaasChatMessage&)>;

DECLARE_MULTICAST_DELEGATE_OneParam(FCYGBaasChatMessageEvent, const FCYGBaasChatMessage&);
DECLARE_MULTICAST_DELEGATE_TwoParams(FCYGBaasChatUpdatedEvent, const FString&, const TArray<FCYGBaasChatMessage>&);

/**
 * Text channels over the control plane's `/v2/channel` routes, with a local
 * timeline cache. Every call is a callback, matching the rest of the BaaS
 * layer; the two static delegates report live arrivals and cache changes.
 * A channel id is the server's dotted four-part id
 * (`<mode>.<subject>.<descriptor>.<label>`); use RoomChannelId,
 * GroupChannelId or DirectChannelId to build one from the ids you have.
 */
class COOKYOURGAME_API FCYGBaasChat
{
public:
	static void History(const FString& ChannelId, const FCYGBaasChatPageResponse& Done,
		int32 Limit = 0, const FString& Cursor = FString(), bool bForward = true);

	static void Send(const FString& ChannelId, const FString& Text, const FCYGBaasChatMessageResponse& Done);
	static void SendContent(const FString& ChannelId, const FString& ContentJson, const FCYGBaasChatMessageResponse& Done);
	static void Edit(const FString& ChannelId, const FString& MessageId, const FString& Text, const FCYGBaasChatMessageResponse& Done);
	static void EditContent(const FString& ChannelId, const FString& MessageId, const FString& ContentJson, const FCYGBaasChatMessageResponse& Done);
	static void Remove(const FString& ChannelId, const FString& MessageId, const FCYGBaasChatMessageResponse& Done);

	/** Feeds a live `apiChannelMessage` payload into the cache; false when it is not a message. */
	static bool Ingest(const FString& MessageJson);

	/** The cached timeline, oldest first; empty when nothing has arrived. */
	static const TArray<FCYGBaasChatMessage>& MessagesFor(const FString& ChannelId);

	/** Drops one channel's cache, or every channel when the id is empty. */
	static void Clear(const FString& ChannelId = FString());

	/** The dotted channel id of a room stream: `2..<room>`. */
	static FString RoomChannelId(const FString& RoomName);

	/** The dotted channel id of a group stream: `3.<group_id>..`. */
	static FString GroupChannelId(const FString& GroupId);

	/** The canonical dotted channel id of a direct stream: `4.<min>.<max>.`. */
	static FString DirectChannelId(const FString& UserIdOne, const FString& UserIdTwo);

	static FCYGBaasChatMessageEvent OnMessageReceived;
	static FCYGBaasChatUpdatedEvent OnMessagesUpdated;

private:
	static FCYGBaasChatMessage ParseMessage(const TSharedPtr<FJsonObject>& Object);
	static void ParsePage(const FString& Body, FCYGBaasChatPage& OutPage);
	static bool Merge(const FString& ChannelId, const FCYGBaasChatMessage& Message);
	static void RemoveCached(const FString& ChannelId, const FString& MessageId);
	static void CompleteWrite(const FString& ChannelId, bool bSuccess, int32 StatusCode,
		const FString& Reply, const FCYGBaasChatMessageResponse& Done);
	static void NotifyUpdated(const FString& ChannelId);
	static void ReportFailure(const TCHAR* What, int32 StatusCode, const FString& Reply);
};
