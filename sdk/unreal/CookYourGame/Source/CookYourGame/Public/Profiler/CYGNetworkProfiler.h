// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Core/CYGOpCodes.h"

/**
 * One committed second of traffic.
 *
 * A window is filled by record calls and moved into the ring when the clock rolls
 * over, so a screen reading the ring always sees finished measurements rather than
 * a number that changed underneath it.
 */
struct FCYGNProfilerSample
{
	/** Payload bytes sent while the window was open, header excluded. */
	int64 BytesSent = 0;

	/** Payload bytes received while the window was open. */
	int64 BytesReceived = 0;

	int32 PacketsSent = 0;
	int32 PacketsReceived = 0;

	/** Frames pumped through the subsystem during the window, which is a rate in hertz. */
	int32 TickCount = 0;

	/** Summed frame deltas, so the average frame cost can be recovered from the count. */
	double TickSumSeconds = 0.0;

	/** Longest single frame observed in the window. */
	float LongestTickSeconds = 0.f;

	/** Engine clock at the moment the window opened. */
	double StartedAtSeconds = 0.0;
};

/**
 * Rolling traffic counters for the room link, written by the recording transport.
 *
 * Every number here comes from watching packets, so nothing in the connection path
 * needs to know this exists, and the sink is off until a game or the editor turns it
 * on: a record call is then one branch, and the storage is allocated once, statically,
 * so a hot path never allocates for the sake of being measured.
 *
 * The ring holds the last minute in one-second windows. Averages and per-opcode totals
 * are taken over what is in the ring, which means a room that has been up for ten
 * seconds reports ten seconds of history rather than a minute of nothing.
 */
class COOKYOURGAME_API FCYGNetworkProfiler
{
public:
	/** One-second windows retained, oldest dropped first. */
	static constexpr int32 WindowCount = 60;

	/** Slots for opcode values, the highest being DespawnObject. */
	static constexpr int32 OpcodeSlots = 22;

	/** False by default: recording is opt-in, per project setting or per session. */
	static bool bEnabled;

	/** Counts one outbound packet; expects the canonical header form. */
	static void RecordSend(ECYGOpCode Op, int32 PayloadBytes);

	/** Counts one inbound packet. */
	static void RecordReceive(ECYGOpCode Op, int32 PayloadBytes);

	/** Notes one frame of the pump, for tick rate and worst frame. */
	static void RecordTick(float DeltaSeconds);

	/** Rolls a finished window into the ring; the subsystem calls it once per frame. */
	static void Pump(double NowSeconds);

	/** Forgets the ring and starts the current window at the given clock. */
	static void Reset(double NowSeconds);

	/** Traffic of the window still open, so a screen can show a live number. */
	static int64 LiveBytesSent();
	static int64 LiveBytesReceived();
	static int32 LivePacketsSent();
	static int32 LivePacketsReceived();

	/** Bytes per second, averaged over the windows in the ring. */
	static double AverageBytesPerSecondOut();
	static double AverageBytesPerSecondIn();

	/** Packets per second, averaged over the windows in the ring. */
	static double AveragePacketsPerSecondOut();
	static double AveragePacketsPerSecondIn();

	/** Frame rate over the ring, and the longest frame in it. */
	static double AverageTickHz();
	static float LongestTickSeconds();

	/** Bytes of one opcode across the ring, in or out. */
	static int64 OpcodeBytes(ECYGOpCode Op, bool bOutbound);

	/** Oldest to newest; outside the filled part the sample is empty. */
	static const FCYGNProfilerSample& Sample(int32 Index);

	/** Windows committed so far, capped at the ring size. */
	static int32 SampleCount();

	 /** Largest single-window byte total, for scaling a graph. */
	static int64 PeakWindowBytes();
};
