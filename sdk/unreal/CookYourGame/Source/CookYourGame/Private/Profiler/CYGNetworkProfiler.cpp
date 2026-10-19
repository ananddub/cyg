// Copyright CookYourGame. All rights reserved.

#include "Profiler/CYGNetworkProfiler.h"

namespace
{
	/** Committed windows, walked from Head for as many as Count holds. */
	FCYGNProfilerSample Windows[FCYGNetworkProfiler::WindowCount];

	int64 OpcodeBytesSent[FCYGNetworkProfiler::WindowCount][FCYGNetworkProfiler::OpcodeSlots];
	int64 OpcodeBytesReceived[FCYGNetworkProfiler::WindowCount][FCYGNetworkProfiler::OpcodeSlots];

	/** Per-opcode bytes of the window still being filled, moved into the ring on commit. */
	int64 CurrentOpcodeSent[FCYGNetworkProfiler::OpcodeSlots];
	int64 CurrentOpcodeReceived[FCYGNetworkProfiler::OpcodeSlots];

	/** Ring slot of the oldest committed window. */
	int32 Head = 0;

	/** Committed windows held, growing to the ring size and then holding there. */
	int32 Count = 0;

	/** The window still being filled. */
	FCYGNProfilerSample Current;

	const FCYGNProfilerSample& EmptySample()
	{
		static const FCYGNProfilerSample Empty;

		return Empty;
	}

	int32 SlotOf(ECYGOpCode Op)
	{
		const int32 Slot = static_cast<int32>(Op);

		return Slot >= 0 && Slot < FCYGNetworkProfiler::OpcodeSlots ? Slot : -1;
	}

	/**
	 * Moves the filled window into the ring and opens the next one.
	 *
	 * A catch-up loop is deliberately absent: if the pump was starved for three
	 * seconds, the measurement that follows is a three-second window, and inventing
	 * two empty ones would draw a traffic graph that never happened.
	 */
	void CommitWindow(double NowSeconds)
	{
		const bool bRingIsFull = Count >= FCYGNetworkProfiler::WindowCount;
		const int32 Slot = bRingIsFull ? Head : (Head + Count) % FCYGNetworkProfiler::WindowCount;

		if (!bRingIsFull)
		{
			++Count;
		}
		else
		{
			// The oldest window is the one being overwritten, so the walk starts one on.
			Head = (Head + 1) % FCYGNetworkProfiler::WindowCount;
		}

		Windows[Slot] = Current;

		for (int32 Index = 0; Index < FCYGNetworkProfiler::OpcodeSlots; ++Index)
		{
			OpcodeBytesSent[Slot][Index] = CurrentOpcodeSent[Index];
			OpcodeBytesReceived[Slot][Index] = CurrentOpcodeReceived[Index];

			CurrentOpcodeSent[Index] = 0;
			CurrentOpcodeReceived[Index] = 0;
		}

		Current = FCYGNProfilerSample();
		Current.StartedAtSeconds = NowSeconds;
	}
}

bool FCYGNetworkProfiler::bEnabled = false;

void FCYGNetworkProfiler::RecordSend(ECYGOpCode Op, int32 PayloadBytes)
{
	if (!bEnabled || PayloadBytes <= 0)
	{
		return;
	}

	Current.BytesSent += PayloadBytes;
	++Current.PacketsSent;

	const int32 Slot = SlotOf(Op);

	if (Slot >= 0)
	{
		CurrentOpcodeSent[Slot] += PayloadBytes;
	}
}

void FCYGNetworkProfiler::RecordReceive(ECYGOpCode Op, int32 PayloadBytes)
{
	if (!bEnabled || PayloadBytes <= 0)
	{
		return;
	}

	Current.BytesReceived += PayloadBytes;
	++Current.PacketsReceived;

	const int32 Slot = SlotOf(Op);

	if (Slot >= 0)
	{
		CurrentOpcodeReceived[Slot] += PayloadBytes;
	}
}

void FCYGNetworkProfiler::RecordTick(float DeltaSeconds)
{
	if (!bEnabled || DeltaSeconds <= 0.f)
	{
		return;
	}

	++Current.TickCount;
	Current.TickSumSeconds += DeltaSeconds;

	if (DeltaSeconds > Current.LongestTickSeconds)
	{
		Current.LongestTickSeconds = DeltaSeconds;
	}
}

void FCYGNetworkProfiler::Pump(double NowSeconds)
{
	if (!bEnabled)
	{
		return;
	}

	if (Current.StartedAtSeconds == 0.0)
	{
		Current.StartedAtSeconds = NowSeconds;
		return;
	}

	if (NowSeconds - Current.StartedAtSeconds >= 1.0)
	{
		CommitWindow(NowSeconds);
	}
}

void FCYGNetworkProfiler::Reset(double NowSeconds)
{
	for (int32 Index = 0; Index < FCYGNetworkProfiler::WindowCount; ++Index)
	{
		Windows[Index] = FCYGNProfilerSample();

		for (int32 Slot = 0; Slot < FCYGNetworkProfiler::OpcodeSlots; ++Slot)
		{
			OpcodeBytesSent[Index][Slot] = 0;
			OpcodeBytesReceived[Index][Slot] = 0;
		}
	}

	for (int32 Slot = 0; Slot < FCYGNetworkProfiler::OpcodeSlots; ++Slot)
	{
		CurrentOpcodeSent[Slot] = 0;
		CurrentOpcodeReceived[Slot] = 0;
	}

	Current = FCYGNProfilerSample();
	Current.StartedAtSeconds = NowSeconds;

	Head = 0;
	Count = 0;
}

int64 FCYGNetworkProfiler::LiveBytesSent()
{
	return Current.BytesSent;
}

int64 FCYGNetworkProfiler::LiveBytesReceived()
{
	return Current.BytesReceived;
}

int32 FCYGNetworkProfiler::LivePacketsSent()
{
	return Current.PacketsSent;
}

int32 FCYGNetworkProfiler::LivePacketsReceived()
{
	return Current.PacketsReceived;
}

double FCYGNetworkProfiler::AverageBytesPerSecondOut()
{
	int64 Total = 0;

	for (int32 Index = 0; Index < Count; ++Index)
	{
		Total += Windows[(Head + Index) % FCYGNetworkProfiler::WindowCount].BytesSent;
	}

	return Count > 0 ? static_cast<double>(Total) / Count : 0.0;
}

double FCYGNetworkProfiler::AverageBytesPerSecondIn()
{
	int64 Total = 0;

	for (int32 Index = 0; Index < Count; ++Index)
	{
		Total += Windows[(Head + Index) % FCYGNetworkProfiler::WindowCount].BytesReceived;
	}

	return Count > 0 ? static_cast<double>(Total) / Count : 0.0;
}

double FCYGNetworkProfiler::AveragePacketsPerSecondOut()
{
	int64 Total = 0;

	for (int32 Index = 0; Index < Count; ++Index)
	{
		Total += Windows[(Head + Index) % FCYGNetworkProfiler::WindowCount].PacketsSent;
	}

	return Count > 0 ? static_cast<double>(Total) / Count : 0.0;
}

double FCYGNetworkProfiler::AveragePacketsPerSecondIn()
{
	int64 Total = 0;

	for (int32 Index = 0; Index < Count; ++Index)
	{
		Total += Windows[(Head + Index) % FCYGNetworkProfiler::WindowCount].PacketsReceived;
	}

	return Count > 0 ? static_cast<double>(Total) / Count : 0.0;
}

double FCYGNetworkProfiler::AverageTickHz()
{
	int64 Ticks = 0;
	double Seconds = 0.0;

	for (int32 Index = 0; Index < Count; ++Index)
	{
		const FCYGNProfilerSample& Window = Windows[(Head + Index) % FCYGNetworkProfiler::WindowCount];
		Ticks += Window.TickCount;
		Seconds += Window.TickSumSeconds;
	}

	return Seconds > 0.0 ? Ticks / Seconds : 0.0;
}

float FCYGNetworkProfiler::LongestTickSeconds()
{
	float Longest = 0.f;

	for (int32 Index = 0; Index < Count; ++Index)
	{
		Longest = FMath::Max(Longest, Windows[(Head + Index) % FCYGNetworkProfiler::WindowCount].LongestTickSeconds);
	}

	return Longest;
}

int64 FCYGNetworkProfiler::OpcodeBytes(ECYGOpCode Op, bool bOutbound)
{
	const int32 Slot = SlotOf(Op);

	if (Slot < 0)
	{
		return 0;
	}

	int64 Total = 0;

	for (int32 Index = 0; Index < Count; ++Index)
	{
		const int32 Window = (Head + Index) % FCYGNetworkProfiler::WindowCount;
		Total += bOutbound ? OpcodeBytesSent[Window][Slot] : OpcodeBytesReceived[Window][Slot];
	}

	return Total;
}

const FCYGNProfilerSample& FCYGNetworkProfiler::Sample(int32 Index)
{
	if (Index < 0 || Index >= Count)
	{
		return EmptySample();
	}

	return Windows[(Head + Index) % FCYGNetworkProfiler::WindowCount];
}

int32 FCYGNetworkProfiler::SampleCount()
{
	return Count;
}

int64 FCYGNetworkProfiler::PeakWindowBytes()
{
	int64 Peak = 0;

	for (int32 Index = 0; Index < Count; ++Index)
	{
		const FCYGNProfilerSample& Window = Windows[(Head + Index) % FCYGNetworkProfiler::WindowCount];
		Peak = FMath::Max(Peak, FMath::Max(Window.BytesSent, Window.BytesReceived));
	}

	return Peak;
}
