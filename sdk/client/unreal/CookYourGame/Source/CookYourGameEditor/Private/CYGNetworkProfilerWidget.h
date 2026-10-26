// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Widgets/SCompoundWidget.h"

/**
 * The room link's traffic, drawn from the counters the recording transport wrote.
 *
 * Every number here is read at paint time, so this widget holds no state at all:
 * closing it, opening it again, or letting it sit for a minute costs nothing and
 * changes nothing, and the readers can be static functions because there is
 * nothing on the instance for them to need. That also means the lambdas the text
 * blocks call capture nothing, so a block cannot outlive what it was reading.
 * The recorder lives in the runtime module and the ring it fills is static, so
 * this screen is a window onto a measurement that keeps running regardless.
 *
 * Two lines of blocks carry the shape of the last minute, one per direction, and
 * both are scaled against the same peak so they can be compared by eye.
 */
class SCYGNetworkProfiler : public SCompoundWidget
{
public:
	SLATE_BEGIN_ARGS(SCYGNetworkProfiler) {}
	SLATE_END_ARGS()

	void Construct(const FArguments& InArgs);

private:
	/** Room, actor number, link state, latency and how many views are live. */
	static FText FormatLink();

	/** Traffic of the open window, then the averages over the ring. */
	static FText FormatRates();

	/** The two block lines, with the peak that scaled them. */
	static FText FormatGraph();

	/** One line per opcode that moved anything in the ring. */
	static FText FormatOpcodes();

	/** Oldest on the left, newest on the right, blank for windows not yet filled. */
	static FString Sparkline(bool bOutbound);

	/** KiB and MiB rather than thousands, because traffic is read in bursts per second. */
	static FString FormatBytes(int64 Bytes);

	FReply HandleReset();
	FReply HandleToggle();
};
