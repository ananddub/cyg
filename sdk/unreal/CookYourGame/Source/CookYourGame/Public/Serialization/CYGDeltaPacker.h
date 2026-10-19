// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"

/**
 * Cuts the difference between two byte blobs the peers already share, and
 * replays that difference. Aimed at snapshots: a room-state blob that changed
 * in three places costs three small runs instead of the whole document.
 *
 * A delta is an LEB128 header followed by records:
 *   [varint baseSize][varint targetSize] then, until the delta ends,
 *   [byte control] where the high bit selects the record:
 *     set   - a literal run of control & 0x7F bytes (1..127), copied verbatim;
 *     clear - a copy of (mode + 2) bytes, or, in long mode, of the three
 *             big-endian bytes that follow, taken from the base at the
 *             big-endian offset in the low five bits of control (that count is
 *             zero when the offset itself is zero).
 * An empty target is the header alone. Encoding never emits more than a whole
 * target copy would cost: if the records grow past that, the delta is thrown
 * away and the target is written as literals.
 *
 * Only the record layout is a contract. Which copies a particular encoder finds
 * is its own business, so a delta made here decodes in any other client of this
 * family without the two encoders agreeing byte for byte.
 *
 * Thread safety: everything is local to the call, so two encodes may run at once.
 */
struct COOKYOURGAME_API FCYGDeltaPacker
{
	/** Delta that rebuilds Target from Base; always returns something. */
	static void CreateDelta(TArrayView<const uint8> Base, TArrayView<const uint8> Target, TArray<uint8>& OutDelta);

	/** Replays a delta. False means the delta is malformed or does not fit the base. */
	static bool TryApplyDelta(TArrayView<const uint8> Base, TArrayView<const uint8> Delta, TArray<uint8>& OutTarget);

	/** Replays a delta, yielding an empty array and a warning when it is malformed. */
	static TArray<uint8> ApplyDelta(TArrayView<const uint8> Base, TArrayView<const uint8> Delta);

	/** Size of header plus literals plus a slack of three bytes per record. */
	static int32 EncodedSize(int32 BaseSize, int32 TargetSize, int32 RecordCount);

	/** Round-trips edge and randomised cases; the report lists any that failed. */
	static bool RunSelfTest(FString& OutReport);
};
