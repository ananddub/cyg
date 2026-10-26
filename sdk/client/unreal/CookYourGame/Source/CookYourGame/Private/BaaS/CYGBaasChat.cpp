// Copyright CookYourGame. All rights reserved.

#include "BaaS/CYGBaasChat.h"

#include "BaaS/CYGBaasHttp.h"
#include "CYGBaasJson.h"
#include "CookYourGame.h"
#include "Core/CYGSettings.h"

namespace
{
	/** channel id -> oldest-first timeline, shared by every caller. */
	TMap<FString, TArray<FCYGBaasChatMessage>> Cache;

	constexpr int32 CacheLimit = 500;

	FString ChannelPath(const FString& ChannelId)
	{
		return FString::Printf(TEXT("/v2/channel/%s"), *UCYGSettings::EscapeSegment(ChannelId));
	}

	FString MessagePath(const FString& ChannelId, const FString& MessageId)
	{
		return FString::Printf(TEXT("%s/message/%s"), *ChannelPath(ChannelId),
			*UCYGSettings::EscapeSegment(MessageId));
	}

	FString ContentBody(const FString& ContentJson)
	{
		return FString::Printf(TEXT("{\"content\":%s}"), *ContentJson);
	}

	FString TextBody(const FString& Text)
	{
		const TSharedRef<FJsonObject> Content = CYGBaasJson::NewObject();
		Content->SetStringField(TEXT("text"), Text);

		return ContentBody(CYGBaasJson::Write(Content));
	}
}

FCYGBaasChatMessageEvent FCYGBaasChat::OnMessageReceived;
FCYGBaasChatUpdatedEvent FCYGBaasChat::OnMessagesUpdated;

void FCYGBaasChat::History(const FString& ChannelId, const FCYGBaasChatPageResponse& Done,
	int32 Limit, const FString& Cursor, bool bForward)
{
	if (ChannelId.IsEmpty())
	{
		if (Done) Done(false, FCYGBaasChatPage());
		return;
	}

	FString Path = ChannelPath(ChannelId);
	TArray<FString> Query;

	if (Limit > 0)
	{
		Query.Add(FString::Printf(TEXT("limit=%d"), Limit));
	}

	if (!Cursor.IsEmpty())
	{
		Query.Add(FString::Printf(TEXT("cursor=%s"), *UCYGSettings::EscapeSegment(Cursor)));
	}

	if (!bForward)
	{
		Query.Add(TEXT("forward=false"));
	}

	if (Query.Num() > 0)
	{
		Path += TEXT("?");
		Path += FString::Join(Query, TEXT("&"));
	}

	FCYGBaasHttp::Get(Path,
		[ChannelId, Done](bool bSuccess, int32 StatusCode, const FString& Reply)
		{
			FCYGBaasChatPage Page;

			if (bSuccess)
			{
				ParsePage(Reply, Page);

				for (const FCYGBaasChatMessage& Message : Page.Messages)
				{
					Merge(ChannelId, Message);
				}

				NotifyUpdated(ChannelId);
			}
			else
			{
				ReportFailure(TEXT("Reading channel history"), StatusCode, Reply);
			}

			if (Done) Done(bSuccess, Page);
		});
}

void FCYGBaasChat::Send(const FString& ChannelId, const FString& Text, const FCYGBaasChatMessageResponse& Done)
{
	SendContent(ChannelId, TextBody(Text), Done);
}

void FCYGBaasChat::SendContent(const FString& ChannelId, const FString& ContentJson, const FCYGBaasChatMessageResponse& Done)
{
	if (ChannelId.IsEmpty() || ContentJson.IsEmpty())
	{
		if (Done) Done(false, FCYGBaasChatMessage());
		return;
	}

	FCYGBaasHttp::Post(FString::Printf(TEXT("%s/message"), *ChannelPath(ChannelId)), ContentBody(ContentJson),
		[ChannelId, Done](bool bSuccess, int32 StatusCode, const FString& Reply)
		{
			CompleteWrite(ChannelId, bSuccess, StatusCode, Reply, Done);
		});
}

void FCYGBaasChat::Edit(const FString& ChannelId, const FString& MessageId, const FString& Text, const FCYGBaasChatMessageResponse& Done)
{
	EditContent(ChannelId, MessageId, TextBody(Text), Done);
}

void FCYGBaasChat::EditContent(const FString& ChannelId, const FString& MessageId, const FString& ContentJson, const FCYGBaasChatMessageResponse& Done)
{
	if (ChannelId.IsEmpty() || MessageId.IsEmpty() || ContentJson.IsEmpty())
	{
		if (Done) Done(false, FCYGBaasChatMessage());
		return;
	}

	FCYGBaasHttp::Put(MessagePath(ChannelId, MessageId), ContentBody(ContentJson),
		[ChannelId, Done](bool bSuccess, int32 StatusCode, const FString& Reply)
		{
			CompleteWrite(ChannelId, bSuccess, StatusCode, Reply, Done);
		});
}

void FCYGBaasChat::Remove(const FString& ChannelId, const FString& MessageId, const FCYGBaasChatMessageResponse& Done)
{
	if (ChannelId.IsEmpty() || MessageId.IsEmpty())
	{
		if (Done) Done(false, FCYGBaasChatMessage());
		return;
	}

	FCYGBaasHttp::Drop(MessagePath(ChannelId, MessageId),
		[ChannelId, MessageId, Done](bool bSuccess, int32 StatusCode, const FString& Reply)
		{
			FCYGBaasChatMessage Message;

			if (bSuccess)
			{
				Message = ParseMessage(CYGBaasJson::Parse(Reply));
				RemoveCached(ChannelId, Message.MessageId.IsEmpty() ? MessageId : Message.MessageId);
				NotifyUpdated(ChannelId);
			}
			else
			{
				ReportFailure(TEXT("Removing a channel message"), StatusCode, Reply);
			}

			if (Done) Done(bSuccess, Message);
		});
}

bool FCYGBaasChat::Ingest(const FString& MessageJson)
{
	const FCYGBaasChatMessage Message = ParseMessage(CYGBaasJson::Parse(MessageJson));

	if (Message.ChannelId.IsEmpty())
	{
		return false;
	}

	Merge(Message.ChannelId, Message);
	OnMessageReceived.Broadcast(Message);
	NotifyUpdated(Message.ChannelId);

	return true;
}

const TArray<FCYGBaasChatMessage>& FCYGBaasChat::MessagesFor(const FString& ChannelId)
{
	static const TArray<FCYGBaasChatMessage> Empty;

	const TArray<FCYGBaasChatMessage>* Found = Cache.Find(ChannelId);

	return Found != nullptr ? *Found : Empty;
}

void FCYGBaasChat::Clear(const FString& ChannelId)
{
	if (ChannelId.IsEmpty())
	{
		Cache.Empty();
	}
	else
	{
		Cache.Remove(ChannelId);
	}
}

FString FCYGBaasChat::RoomChannelId(const FString& RoomName)
{
	return FString::Printf(TEXT("2..%s"), *RoomName);
}

FString FCYGBaasChat::GroupChannelId(const FString& GroupId)
{
	return FString::Printf(TEXT("3.%s.."), *GroupId.ToLower());
}

FString FCYGBaasChat::DirectChannelId(const FString& UserIdOne, const FString& UserIdTwo)
{
	const FString One = UserIdOne.ToLower();
	const FString Two = UserIdTwo.ToLower();
	const bool bOneFirst = One.Compare(Two, ESearchCase::CaseSensitive) <= 0;

	return bOneFirst
		? FString::Printf(TEXT("4.%s.%s."), *One, *Two)
		: FString::Printf(TEXT("4.%s.%s."), *Two, *One);
}

FString FCYGBaasChatMessage::GetText() const
{
	const TSharedPtr<FJsonObject> Object = CYGBaasJson::Parse(Content);

	if (Object.IsValid() && Object->HasField(TEXT("text")))
	{
		return CYGBaasJson::ReadText(Object, TEXT("text"));
	}

	return Content;
}

FCYGBaasChatMessage FCYGBaasChat::ParseMessage(const TSharedPtr<FJsonObject>& Object)
{
	FCYGBaasChatMessage Message;

	if (!Object.IsValid())
	{
		return Message;
	}

	Message.ChannelId = CYGBaasJson::ReadText(Object, TEXT("channel_id"));
	Message.MessageId = CYGBaasJson::ReadText(Object, TEXT("message_id"));
	Message.Code = static_cast<int32>(CYGBaasJson::ReadInt(Object, TEXT("code")));
	Message.SenderId = CYGBaasJson::ReadText(Object, TEXT("sender_id"));
	Message.Username = CYGBaasJson::ReadText(Object, TEXT("username"));
	Message.Content = CYGBaasJson::ReadText(Object, TEXT("content"));
	Message.CreateTime = CYGBaasJson::ReadText(Object, TEXT("create_time"));
	Message.UpdateTime = CYGBaasJson::ReadText(Object, TEXT("update_time"));
	Message.bPersistent = CYGBaasJson::ReadBool(Object, TEXT("persistent"));
	Message.RoomName = CYGBaasJson::ReadText(Object, TEXT("room_name"));
	Message.GroupId = CYGBaasJson::ReadText(Object, TEXT("group_id"));
	Message.UserIdOne = CYGBaasJson::ReadText(Object, TEXT("user_id_one"));
	Message.UserIdTwo = CYGBaasJson::ReadText(Object, TEXT("user_id_two"));

	return Message;
}

void FCYGBaasChat::ParsePage(const FString& Body, FCYGBaasChatPage& OutPage)
{
	const TSharedPtr<FJsonObject> Object = CYGBaasJson::Parse(Body);

	if (!Object.IsValid())
	{
		return;
	}

	for (const TSharedPtr<FJsonValue>& Element : CYGBaasJson::ReadArray(Object, TEXT("messages")))
	{
		OutPage.Messages.Add(ParseMessage(CYGBaasJson::ReadElement(Element)));
	}

	OutPage.NextCursor = CYGBaasJson::ReadText(Object, TEXT("next_cursor"));
	OutPage.PrevCursor = CYGBaasJson::ReadText(Object, TEXT("prev_cursor"));
	OutPage.CacheableCursor = CYGBaasJson::ReadText(Object, TEXT("cacheable_cursor"));
}

bool FCYGBaasChat::Merge(const FString& ChannelId, const FCYGBaasChatMessage& Message)
{
	TArray<FCYGBaasChatMessage>& Messages = Cache.FindOrAdd(ChannelId);

	for (int32 Index = 0; Index < Messages.Num(); ++Index)
	{
		if (Messages[Index].MessageId == Message.MessageId)
		{
			Messages[Index] = Message;
			return false;
		}
	}

	Messages.Add(Message);

	if (Messages.Num() > CacheLimit)
	{
		Messages.RemoveAt(0, Messages.Num() - CacheLimit);
	}

	return true;
}

void FCYGBaasChat::RemoveCached(const FString& ChannelId, const FString& MessageId)
{
	TArray<FCYGBaasChatMessage>* Messages = Cache.Find(ChannelId);

	if (Messages == nullptr)
	{
		return;
	}

	Messages->RemoveAll([&MessageId](const FCYGBaasChatMessage& Message)
	{
		return Message.MessageId == MessageId;
	});
}

void FCYGBaasChat::CompleteWrite(const FString& ChannelId, bool bSuccess, int32 StatusCode,
	const FString& Reply, const FCYGBaasChatMessageResponse& Done)
{
	FCYGBaasChatMessage Message;

	if (bSuccess)
	{
		Message = ParseMessage(CYGBaasJson::Parse(Reply));

		if (!Message.MessageId.IsEmpty())
		{
			Merge(ChannelId, Message);
			OnMessageReceived.Broadcast(Message);
			NotifyUpdated(ChannelId);
		}
		else
		{
			bSuccess = false;
			ReportFailure(TEXT("Reading the written channel message"), StatusCode, Reply);
		}
	}
	else
	{
		ReportFailure(TEXT("Writing a channel message"), StatusCode, Reply);
	}

	if (Done) Done(bSuccess, Message);
}

void FCYGBaasChat::NotifyUpdated(const FString& ChannelId)
{
	const TArray<FCYGBaasChatMessage> Snapshot = MessagesFor(ChannelId);
	OnMessagesUpdated.Broadcast(ChannelId, Snapshot);
}

void FCYGBaasChat::ReportFailure(const TCHAR* What, int32 StatusCode, const FString& Reply)
{
	UE_LOG(LogCookYourGame, Error, TEXT("%s failed (%d): %s"), What, StatusCode,
		*CYGBaasJson::ReadError(CYGBaasJson::Parse(Reply)));
}
