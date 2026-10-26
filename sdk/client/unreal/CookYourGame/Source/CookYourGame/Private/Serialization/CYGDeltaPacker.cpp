// Copyright CookYourGame. All rights reserved.

#include "Serialization/CYGDeltaPacker.h"

#include "CookYourGame.h"

namespace
{
	/** Bytes a fingerprint covers: short enough to hit, long enough to be rare. */
	constexpr int32 WindowBytes = 16;
	constexpr int32 HashSlots = 4096;
	constexpr int32 SearchCap = 0x400000;
	constexpr int32 BaseCap = 0x10000000;
	constexpr int32 TargetCap = 0x40000000;
	constexpr int32 LongMode = 3;
	constexpr int32 MaxLiteralRun = 127;

	struct FWindow
	{
		int32 Offset = -1;
		int32 Sum = 0;
		int32 Weighted = 0;
	};

	int32 VarintBytes(uint32 Value)
	{
		int32 Count = 1;

		while (Value >= 0x80u)
		{
			Value >>= 7;
			++Count;
		}

		return Count;
	}

	void AppendVarint(TArray<uint8>& Out, uint32 Value)
	{
		while (Value >= 0x80u)
		{
			Out.Add(static_cast<uint8>(Value | 0x80u));
			Value >>= 7;
		}

		Out.Add(static_cast<uint8>(Value));
	}

	bool ReadVarint(const uint8* Data, int32 Size, int32& Cursor, uint32& OutValue)
	{
		OutValue = 0;
		int32 Shift = 0;

		for (;;)
		{
			if (Cursor >= Size)
			{
				return false;
			}

			const uint8 Byte = Data[Cursor++];
			OutValue |= static_cast<uint32>(Byte & 0x7F) << Shift;

			if ((Byte & 0x80) == 0)
			{
				return true;
			}

			Shift += 7;

			if (Shift > 31)
			{
				return false;
			}
		}
	}

	/** Rolling fingerprint of the window at Start: a plain and a position-weighted sum. */
	void MeasureWindow(const uint8* Data, int32 Start, int32& OutSum, int32& OutWeighted)
	{
		OutSum = 0;
		OutWeighted = 0;

		for (int32 Offset = 0; Offset < WindowBytes; ++Offset)
		{
			const int32 Byte = Data[Start + Offset];
			OutSum += Byte;
			OutWeighted += (Offset + 1) * Byte;
		}
	}

	int32 CombineHash(int32 Sum, int32 Weighted)
	{
		// Wraparound is deliberate: the two constants are the Knuth and small-prime
		// mixers, and only the low bits of the result index a slot.
		const uint32 Mixed = static_cast<uint32>(Sum) * 2654435761u + static_cast<uint32>(Weighted) * 40503u;

		return static_cast<int32>(Mixed & static_cast<uint32>(HashSlots - 1));
	}

	void IndexBase(const uint8* Base, int32 HashBound, TArray<FWindow>& Slots)
	{
		Slots.Init(FWindow(), HashSlots);
		int32 Sum = 0;
		int32 Weighted = 0;
		MeasureWindow(Base, 0, Sum, Weighted);

		FWindow& First = Slots[CombineHash(Sum, Weighted)];
		First.Sum = Sum;
		First.Weighted = Weighted;
		First.Offset = 0;

		for (int32 Start = 1; Start < HashBound; ++Start)
		{
			const int32 Left = Base[Start - 1];
			const int32 Right = Base[Start + WindowBytes - 1];

			// Shifting the weighted sum by the whole previous sum is what makes the
			// fingerprint position sensitive without recomputing sixteen products.
			Weighted -= Sum;
			Sum += Right - Left;
			Weighted += WindowBytes * Right;

			FWindow& Slot = Slots[CombineHash(Sum, Weighted)];

			// The earliest window keeps its slot, so equal fingerprints resolve to the
			// first place the pattern appeared, which is the cheapest offset to encode.
			if (Slot.Offset >= 0 && Slot.Sum == Sum && Slot.Weighted == Weighted)
			{
				continue;
			}

			Slot.Sum = Sum;
			Slot.Weighted = Weighted;
			Slot.Offset = Start;
		}
	}

	void AppendLiterals(TArray<uint8>& Out, const uint8* Target, int32 From, int32 To)
	{
		for (int32 Start = From; Start < To; )
		{
			const int32 Run = FMath::Min(To - Start, MaxLiteralRun);
			Out.Add(static_cast<uint8>(0x80 | Run));
			Out.Append(Target + Start, Run);
			Start += Run;
		}
	}

	void AppendCopy(TArray<uint8>& Out, int32 Offset, int32 Length)
	{
		int32 OffsetBytes = 0;

		while ((static_cast<uint32>(Offset) >> (8 * OffsetBytes)) != 0u && OffsetBytes < 4)
		{
			++OffsetBytes;
		}

		const bool bLong = Length >= 5;
		const int32 Control = (bLong ? (LongMode << 5) : ((Length - 2) << 5)) | OffsetBytes;

		Out.Add(static_cast<uint8>(Control));

		if (bLong)
		{
			Out.Add(static_cast<uint8>(Length >> 16));
			Out.Add(static_cast<uint8>(Length >> 8));
			Out.Add(static_cast<uint8>(Length));
		}

		for (int32 Shift = (OffsetBytes - 1) * 8; Shift >= 0; Shift -= 8)
		{
			Out.Add(static_cast<uint8>(Offset >> Shift));
		}
	}

	/** True while the delta still costs less than writing the target out raw. */
	bool WorthKeeping(const TArray<uint8>& Delta, int32 Capacity, int32 LiteralSize)
	{
		return Delta.Num() <= Capacity && Delta.Num() < LiteralSize;
	}

	bool EncodeMatched(const uint8* Base, int32 BaseSize, const uint8* Target, int32 TargetSize, TArray<uint8>& Out)
	{
		const int32 HashBound = FMath::Min(BaseSize - WindowBytes + 1, SearchCap);

		TArray<FWindow> Slots;
		IndexBase(Base, HashBound, Slots);

		const int32 Capacity = TargetSize + TargetSize / 4 + 64;
		const int32 LiteralSize = TargetSize + VarintBytes(static_cast<uint32>(BaseSize)) + VarintBytes(static_cast<uint32>(TargetSize)) + 16;

		Out.Empty(Capacity);
		AppendVarint(Out, static_cast<uint32>(BaseSize));
		AppendVarint(Out, static_cast<uint32>(TargetSize));

		if (TargetSize == 0)
		{
			return true;
		}

		int32 Cursor = 0;
		int32 LiteralStart = 0;
		int32 Sum = 0;
		int32 Weighted = 0;
		bool bWindowValid = false;

		while (Cursor + WindowBytes <= TargetSize)
		{
			if (bWindowValid)
			{
				const int32 Left = Target[Cursor - 1];
				const int32 Right = Target[Cursor + WindowBytes - 1];

				Weighted -= Sum;
				Sum += Right - Left;
				Weighted += WindowBytes * Right;
			}
			else
			{
				MeasureWindow(Target, Cursor, Sum, Weighted);
				bWindowValid = true;
			}

			const FWindow& Slot = Slots[CombineHash(Sum, Weighted)];
			int32 Candidate = -1;

			if (Slot.Offset >= 0 && Slot.Offset < HashBound
				&& FMemory::Memcmp(Base + Slot.Offset, Target + Cursor, WindowBytes) == 0)
			{
				Candidate = Slot.Offset;
			}

			if (Candidate < 0)
			{
				++Cursor;
				continue;
			}

			// The fingerprint only promises sixteen bytes, so the run grows from there.
			int32 Length = WindowBytes;

			while (Cursor + Length < TargetSize && Candidate + Length < BaseSize
				&& Base[Candidate + Length] == Target[Cursor + Length])
			{
				++Length;
			}

			AppendLiterals(Out, Target, LiteralStart, Cursor);
			AppendCopy(Out, Candidate, Length);

			if (Out.Num() > Capacity)
			{
				return false;
			}

			Cursor += Length;
			LiteralStart = Cursor;
			bWindowValid = false;
		}

		AppendLiterals(Out, Target, LiteralStart, TargetSize);

		return WorthKeeping(Out, Capacity, LiteralSize);
	}

	void EncodeLiterals(int32 BaseSize, const uint8* Target, int32 TargetSize, TArray<uint8>& Out)
	{
		Out.Empty(TargetSize + TargetSize / 4 + 64);
		AppendVarint(Out, static_cast<uint32>(BaseSize));
		AppendVarint(Out, static_cast<uint32>(TargetSize));
		AppendLiterals(Out, Target, 0, TargetSize);
	}
}

void FCYGDeltaPacker::CreateDelta(TArrayView<const uint8> Base, TArrayView<const uint8> Target, TArray<uint8>& OutDelta)
{
	const uint8* TargetData = Target.GetData();
	const uint8* BaseData = Base.GetData();

	// Both guards matter: a huge base would index a fingerprint table that stops short of it,
	// and a small one cannot even fill a window.
	if (Target.Num() > TargetCap || Base.Num() < WindowBytes || Base.Num() > BaseCap)
	{
		EncodeLiterals(Base.Num(), TargetData, Target.Num(), OutDelta);
		return;
	}

	if (!EncodeMatched(BaseData, Base.Num(), TargetData, Target.Num(), OutDelta))
	{
		EncodeLiterals(Base.Num(), TargetData, Target.Num(), OutDelta);
	}
}

bool FCYGDeltaPacker::TryApplyDelta(TArrayView<const uint8> Base, TArrayView<const uint8> Delta, TArray<uint8>& OutTarget)
{
	OutTarget.Reset();

	if (Delta.Num() <= 0)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("The delta stream is empty, so there is nothing to replay."));
		return false;
	}

	const uint8* Source = Delta.GetData();
	const int32 Size = Delta.Num();
	const uint8* BaseData = Base.GetData();
	const int32 BaseSize = Base.Num();

	int32 Cursor = 0;
	uint32 DeclaredBase = 0;
	uint32 DeclaredTarget = 0;

	if (!ReadVarint(Source, Size, Cursor, DeclaredBase))
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("A delta's base-size varint ran off the end of the stream."));
		return false;
	}

	if (!ReadVarint(Source, Size, Cursor, DeclaredTarget))
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("A delta's target-size varint ran off the end of the stream."));
		return false;
	}

	if (DeclaredTarget > static_cast<uint32>(TargetCap))
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("A delta claims a target of %u bytes, past the guard."), DeclaredTarget);
		return false;
	}

	const int32 TargetSize = static_cast<int32>(DeclaredTarget);

	// Zero-filled because a malformed stream may finish short and the caller only learns that
	// from the return value; the bytes themselves are never read on that path.
	OutTarget.SetNumZeroed(TargetSize);

	int32 Produced = 0;

	while (Cursor < Size)
	{
		const uint8 Control = Source[Cursor++];

		if ((Control & 0x80) != 0)
		{
			const int32 Run = Control & 0x7F;

			if (Run == 0 || Run > Size - Cursor || Run > TargetSize - Produced)
			{
				UE_LOG(LogCookYourGame, Warning, TEXT("A literal run reads past the delta or overflows the target."));
				OutTarget.Reset();
				return false;
			}

			FMemory::Memcpy(OutTarget.GetData() + Produced, Source + Cursor, Run);
			Cursor += Run;
			Produced += Run;
			continue;
		}

		const int32 Mode = Control >> 5;
		const int32 OffsetBytes = Control & 31;
		int32 Length = Mode + 2;

		if (Mode == LongMode)
		{
			if (Cursor + 3 > Size)
			{
				UE_LOG(LogCookYourGame, Warning, TEXT("A long copy lost its length bytes."));
				OutTarget.Reset();
				return false;
			}

			Length = (Source[Cursor] << 16) | (Source[Cursor + 1] << 8) | Source[Cursor + 2];
			Cursor += 3;

			if (Length < 5)
			{
				UE_LOG(LogCookYourGame, Warning, TEXT("A long copy declared %d bytes, which short mode already covers."), Length);
				OutTarget.Reset();
				return false;
			}
		}

		// An encoder names the offset in as many bytes as it needs, so more than four
		// cannot come from one, and no base that size exists to read from.
		if (OffsetBytes > 4 || OffsetBytes > Size - Cursor)
		{
			UE_LOG(LogCookYourGame, Warning, TEXT("A copy lost its offset bytes."));
			OutTarget.Reset();
			return false;
		}

		uint32 Offset = 0;

		for (int32 Index = 0; Index < OffsetBytes; ++Index)
		{
			Offset = (Offset << 8) | Source[Cursor++];
		}

		// The window has to sit wholly inside the base the peers agreed on.
		if (Offset > static_cast<uint32>(BaseSize) || Length > BaseSize - static_cast<int32>(Offset)
			|| Length > TargetSize - Produced)
		{
			UE_LOG(LogCookYourGame, Warning, TEXT("A copy reaches outside the base (offset %u, length %d of %d)."), Offset, Length, BaseSize);
			OutTarget.Reset();
			return false;
		}

		FMemory::Memcpy(OutTarget.GetData() + Produced, BaseData + Offset, Length);
		Produced += Length;
	}

	if (Produced != TargetSize)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("A delta rebuilt %d bytes where its header promised %d."), Produced, TargetSize);
		OutTarget.Reset();
		return false;
	}

	return true;
}

TArray<uint8> FCYGDeltaPacker::ApplyDelta(TArrayView<const uint8> Base, TArrayView<const uint8> Delta)
{
	TArray<uint8> Target;
	TryApplyDelta(Base, Delta, Target);

	return Target;
}

int32 FCYGDeltaPacker::EncodedSize(int32 BaseSize, int32 TargetSize, int32 RecordCount)
{
	int32 Size = VarintBytes(static_cast<uint32>(BaseSize)) + VarintBytes(static_cast<uint32>(TargetSize));

	if (TargetSize > 0)
	{
		Size += (TargetSize / MaxLiteralRun) * (1 + MaxLiteralRun);

		const int32 Tail = TargetSize % MaxLiteralRun;

		if (Tail != 0)
		{
			Size += 1 + Tail;
		}
	}

	return Size + FMath::Max(0, RecordCount) * 3;
}

bool FCYGDeltaPacker::RunSelfTest(FString& OutReport)
{
	auto RoundTrip = [](TArrayView<const uint8> Base, TArrayView<const uint8> Target, const FString& Name, int32& Cases, bool& bAllPassed, FString& Report)
	{
		++Cases;

		TArray<uint8> Delta;
		CreateDelta(Base, Target, Delta);

		TArray<uint8> Rebuilt;

		if (!TryApplyDelta(Base, Delta, Rebuilt)
			|| Rebuilt.Num() != Target.Num()
			|| (Target.Num() > 0 && FMemory::Memcmp(Rebuilt.GetData(), Target.GetData(), Target.Num()) != 0))
		{
			bAllPassed = false;
			Report += FString::Printf(TEXT("FAIL: %s\r\n"), *Name);
		}
	};

	bool bAllPassed = true;
	int32 Cases = 0;
	OutReport.Reset();

	const uint8 Single[1] = { 0 };
	const uint8 SmallRaw[] = { 1, 2, 3, 2, 1, 4, 4, 4, 1, 2, 3, 9, 9 };
	const TArrayView<const uint8> Nothing(Single, 0);
	const TArrayView<const uint8> Small(SmallRaw, UE_ARRAY_COUNT(SmallRaw));

	TArray<uint8> Zeros;
	Zeros.Init(0, 17);

	TArray<uint8> Sevens;
	Sevens.Init(7, 17);

	TArray<uint8> Ramp;
	Ramp.SetNum(4096);

	for (int32 Index = 0; Index < Ramp.Num(); ++Index)
	{
		Ramp[Index] = static_cast<uint8>(Index & 0xFF);
	}

	TArray<uint8> Edited = Ramp;
	Edited[10] = 0xEE;
	Edited[1000] = 0xFF;
	Edited[3000] = 0x11;

	TArray<uint8> Shifted;
	Shifted.SetNumZeroed(4106);
	FMemory::Memcpy(Shifted.GetData() + 10, Ramp.GetData(), Ramp.Num());

	RoundTrip(Nothing, Nothing, TEXT("empty against empty"), Cases, bAllPassed, OutReport);
	RoundTrip(Nothing, Small, TEXT("no shared base"), Cases, bAllPassed, OutReport);
	RoundTrip(Small, Small, TEXT("identical blobs"), Cases, bAllPassed, OutReport);
	RoundTrip(Small, TArrayView<const uint8>(SmallRaw, 3), TEXT("base longer than target"), Cases, bAllPassed, OutReport);
	RoundTrip(Zeros, Sevens, TEXT("nothing in common"), Cases, bAllPassed, OutReport);
	RoundTrip(Ramp, Edited, TEXT("an edited snapshot"), Cases, bAllPassed, OutReport);
	RoundTrip(Ramp, Shifted, TEXT("a shifted snapshot"), Cases, bAllPassed, OutReport);

	FRandomStream Stream(12345);
	TArray<uint8> Left;
	TArray<uint8> Right;

	for (int32 Case = 0; Case < 200; ++Case)
	{
		Left.SetNum(Stream.RandRange(0, 299));
		Right.SetNum(Stream.RandRange(0, 299));

		// Four symbols keep repeats frequent enough that copies actually get emitted.
		for (uint8& Byte : Left) { Byte = static_cast<uint8>(Stream.RandRange(0, 3)); }
		for (uint8& Byte : Right) { Byte = static_cast<uint8>(Stream.RandRange(0, 3)); }

		RoundTrip(Left, Right, FString::Printf(TEXT("random#%d"), Case), Cases, bAllPassed, OutReport);
	}

	OutReport += FString::Printf(TEXT("CYGDeltaPacker self-test: %d cases, %s."),
		Cases, bAllPassed ? TEXT("all passed") : TEXT("failures present"));

	return bAllPassed;
}
