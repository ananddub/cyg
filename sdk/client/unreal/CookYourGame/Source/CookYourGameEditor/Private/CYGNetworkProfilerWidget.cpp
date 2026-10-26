// Copyright CookYourGame. All rights reserved.

#include "CYGNetworkProfilerWidget.h"

#include "CYGActiveLink.h"
#include "CYGLiveText.h"
#include "Core/CYGClient.h"
#include "Core/CYGOpCodes.h"
#include "Core/CYGSettings.h"
#include "Gameplay/CYGView.h"
#include "HAL/PlatformTime.h"
#include "Profiler/CYGNetworkProfiler.h"
#include "Widgets/Input/SButton.h"
#include "Widgets/SBoxPanel.h"

#define LOCTEXT_NAMESPACE "CYGEditor"

namespace
{
	/**
	 * What each opcode is called on this screen.
	 *
	 * Spelled out instead of pulled from the enum's reflection because the labels
	 * are for humans reading a graph, and reading them should not depend on how a
	 * given engine version chose to store blueprint enum metadata.
	 */
	struct FOpcodeRow
	{
		ECYGOpCode Op;
		const TCHAR* Label;
	};

	const FOpcodeRow OpcodeRows[] =
	{
		{ ECYGOpCode::Ping, TEXT("Ping") },
		{ ECYGOpCode::Pong, TEXT("Pong") },
		{ ECYGOpCode::JoinRoom, TEXT("Join room") },
		{ ECYGOpCode::LeaveRoom, TEXT("Leave room") },
		{ ECYGOpCode::PlayerInput, TEXT("Player input") },
		{ ECYGOpCode::StateDelta, TEXT("State delta") },
		{ ECYGOpCode::CustomRpc, TEXT("Rpc") },
		{ ECYGOpCode::RewindHit, TEXT("Rewind hit") },
		{ ECYGOpCode::StateSnapshot, TEXT("State snapshot") },
		{ ECYGOpCode::ProtoJoinRoom, TEXT("Wire join") },
		{ ECYGOpCode::ProtoLeaveRoom, TEXT("Wire leave") },
		{ ECYGOpCode::ProtoRoomData, TEXT("Room event") },
		{ ECYGOpCode::ProtoRoomState, TEXT("Room state") },
		{ ECYGOpCode::ProtoStatePatch, TEXT("State patch") },
		{ ECYGOpCode::ProtoDataBytes, TEXT("Room bytes") },
		{ ECYGOpCode::SpawnObject, TEXT("Spawn actor") },
		{ ECYGOpCode::DespawnObject, TEXT("Despawn actor") },
	};

	/** The eight Unicode block shades, one character per second of history. */
	constexpr WCHAR BarFirst = 0x2581;
	constexpr WCHAR BarLast = 0x2588;

	int32 BarLevelFor(const int64 Bytes, const int64 Peak)
	{
		if (Bytes <= 0 || Peak <= 0)
		{
			return 0;
		}

		// Anything that moved is at least the shortest bar, so a quiet second with one
		// heartbeat in it still shows rather than reading as a gap.
		const int32 Spread = static_cast<int32>(BarLast - BarFirst);

		return FMath::Clamp(static_cast<int32>((Bytes * Spread) / Peak), 1, Spread);
	}
}

void SCYGNetworkProfiler::Construct(const FArguments& InArgs)
{
	ChildSlot
	[
		SNew(SVerticalBox)

		+ SVerticalBox::Slot()
		.AutoHeight()
		.Padding(10.f)
		[
			FCYGLiveText::Make([]() { return FormatLink(); })
		]

		+ SVerticalBox::Slot()
		.AutoHeight()
		.Padding(10.f)
		[
			FCYGLiveText::Make([]() { return FormatRates(); })
		]

		+ SVerticalBox::Slot()
		.AutoHeight()
		.Padding(10.f)
		[
			FCYGLiveText::Make([]() { return FormatGraph(); })
		]

		+ SVerticalBox::Slot()
		.AutoHeight()
		.Padding(10.f)
		[
			SNew(SHorizontalBox)

			+ SHorizontalBox::Slot()
			.AutoWidth()
			.Padding(6.f)
			[
				SNew(SButton)
				.Text(LOCTEXT("ResetRing", "Reset ring"))
				.ToolTipText(LOCTEXT("ResetRingTip", "Forgets the windows collected so far and starts the current one again."))
				.OnClicked(this, &SCYGNetworkProfiler::HandleReset)
			]

			+ SHorizontalBox::Slot()
			.AutoWidth()
			[
				SNew(SButton)
				.Text(LOCTEXT("ToggleRecording", "Toggle recording"))
				.ToolTipText(LOCTEXT("ToggleRecordingTip",
					"Turns counting on or off and writes the project setting. Off stops at once; on starts when the link next opens, because counting lives in the transport the link was built with."))
				.OnClicked(this, &SCYGNetworkProfiler::HandleToggle)
			]
		]

		+ SVerticalBox::Slot()
		.FillHeight(1.f)
		.Padding(10.f)
		[
			FCYGLiveText::Make([]() { return FormatOpcodes(); })
		]
	];
}

FText SCYGNetworkProfiler::FormatLink()
{
	const UCYGClient* Client = FCYGActiveLink::Find();

	if (Client == nullptr)
	{
		return FText::FromString(TEXT("No room client. Start a play session, or connect one from a Blueprint."));
	}

	FString Line = FString::Printf(TEXT("room %s   actor %d   %s   link %s   rtt %.1f ms"),
		*Client->GetRoomName(),
		Client->GetLocalActorNumber(),
		Client->IsMasterClient() ? TEXT("master") : TEXT("peer"),
		Client->IsLinkConnected() ? TEXT("up") : TEXT("down"),
		Client->GetLatencyMilliseconds());

	if (!Client->IsLinkConnected())
	{
		// A close code is only interesting when there is no link to explain itself,
		// and it is the one number that says why a resume loop is still running.
		Line += FString::Printf(TEXT("   last close %d"), Client->GetLastErrorCloseCode());
	}

	Line += FString::Printf(TEXT("   views %d   recording %s"),
		FCYGViewTable::Count(),
		FCYGNetworkProfiler::bEnabled ? TEXT("on") : TEXT("off"));

	return FText::FromString(Line);
}

FText SCYGNetworkProfiler::FormatRates()
{
	const int32 Filled = FCYGNetworkProfiler::SampleCount();

	FString Line = FString::Printf(TEXT("now  out %s   in %s   pkt %d / %d"),
		*FormatBytes(FCYGNetworkProfiler::LiveBytesSent()),
		*FormatBytes(FCYGNetworkProfiler::LiveBytesReceived()),
		FCYGNetworkProfiler::LivePacketsSent(),
		FCYGNetworkProfiler::LivePacketsReceived());

	if (Filled > 0)
	{
		// Averages over what is in the ring, so a room that has been up for ten
		// seconds reports ten seconds and not a minute of silence.
		Line += FString::Printf(TEXT("\naveraged over %d s  out %s/s   in %s/s   %s/s out   %s/s in"),
			Filled,
			*FormatBytes(static_cast<int64>(FCYGNetworkProfiler::AverageBytesPerSecondOut())),
			*FormatBytes(static_cast<int64>(FCYGNetworkProfiler::AverageBytesPerSecondIn())),
			*FString::Printf(TEXT("%.1f pkt"), FCYGNetworkProfiler::AveragePacketsPerSecondOut()),
			*FString::Printf(TEXT("%.1f pkt"), FCYGNetworkProfiler::AveragePacketsPerSecondIn()));
	}

	Line += FString::Printf(TEXT("\npump %.1f Hz   worst frame %.1f ms"),
		FCYGNetworkProfiler::AverageTickHz(),
		FCYGNetworkProfiler::LongestTickSeconds() * 1000.0);

	return FText::FromString(Line);
}

FText SCYGNetworkProfiler::FormatGraph()
{
	const int64 Peak = FCYGNetworkProfiler::PeakWindowBytes();

	const FString Line = FString::Printf(TEXT("out %s\nin  %s\npeak %s per second"),
		*Sparkline(true),
		*Sparkline(false),
		*FormatBytes(Peak));

	return FText::FromString(Line);
}

FText SCYGNetworkProfiler::FormatOpcodes()
{
	const int32 Filled = FMath::Max(1, FCYGNetworkProfiler::SampleCount());
	int32 Rows = 0;
	FString Table;

	for (const FOpcodeRow& Row : OpcodeRows)
	{
		const int64 Sent = FCYGNetworkProfiler::OpcodeBytes(Row.Op, true);
		const int64 Received = FCYGNetworkProfiler::OpcodeBytes(Row.Op, false);

		if (Sent <= 0 && Received <= 0)
		{
			continue;
		}

		Table += FString::Printf(TEXT("%s  out %s   in %s   (%s/s and %s/s)\n"),
			*FString(Row.Label).LeftJustify(15, TEXT(' ')),
			*FormatBytes(Sent),
			*FormatBytes(Received),
			*FormatBytes(Sent / Filled),
			*FormatBytes(Received / Filled));

		++Rows;
	}

	if (Rows == 0)
	{
		return FText::FromString(TEXT("Nothing counted yet. Recording is either off or has not seen a packet."));
	}

	return FText::FromString(Table);
}

FString SCYGNetworkProfiler::Sparkline(const bool bOutbound)
{
	const int64 Peak = FCYGNetworkProfiler::PeakWindowBytes();
	const int32 Filled = FCYGNetworkProfiler::SampleCount();
	FString Line;

	Line.Reserve(FCYGNetworkProfiler::WindowCount);

	// Time runs to the right, so the unfilled part of the ring is blank on the left
	// rather than a flat line that would read as a drop to zero.
	for (int32 Index = Filled; Index < FCYGNetworkProfiler::WindowCount; ++Index)
	{
		Line.AppendChar(TEXT(' '));
	}

	for (int32 Index = 0; Index < Filled; ++Index)
	{
		const FCYGNProfilerSample& Entry = FCYGNetworkProfiler::Sample(Index);
		const int64 Bytes = bOutbound ? Entry.BytesSent : Entry.BytesReceived;

		Line.AppendChar(static_cast<WCHAR>(BarFirst + BarLevelFor(Bytes, Peak)));
	}

	return Line;
}

FString SCYGNetworkProfiler::FormatBytes(const int64 Bytes)
{
	if (Bytes < 1024)
	{
		return FString::Printf(TEXT("%lld B"), Bytes);
	}

	if (Bytes < 1024 * 1024)
	{
		return FString::Printf(TEXT("%.1f KiB"), static_cast<double>(Bytes) / 1024.0);
	}

	return FString::Printf(TEXT("%.2f MiB"), static_cast<double>(Bytes) / (1024.0 * 1024.0));
}

FReply SCYGNetworkProfiler::HandleReset()
{
	FCYGNetworkProfiler::Reset(FPlatformTime::Seconds());

	return FReply::Handled();
}

FReply SCYGNetworkProfiler::HandleToggle()
{
	// The switch is the runtime's own, so it also decides whether the next link is
	// built with the counting transport, and the project setting is written to match
	// it: a knob that only lasted until the editor closed would leave the two
	// disagreeing about what the next session does.
	FCYGNetworkProfiler::bEnabled = !FCYGNetworkProfiler::bEnabled;

	if (UCYGSettings* Config = GetMutableDefault<UCYGSettings>())
	{
		Config->bProfileTraffic = FCYGNetworkProfiler::bEnabled;
		Config->SaveConfig();
	}

	if (!FCYGNetworkProfiler::bEnabled)
	{
		FCYGNetworkProfiler::Reset(FPlatformTime::Seconds());
	}

	return FReply::Handled();
}

#undef LOCTEXT_NAMESPACE
