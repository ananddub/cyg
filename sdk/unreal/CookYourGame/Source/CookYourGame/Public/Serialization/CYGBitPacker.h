// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"

/**
 * Grows a packet byte by byte. Integers use LEB128 (zig-zag when signed),
 * floats are written big-endian, and the two fixed-width vector and quaternion
 * forms are documented on their methods.
 *
 * The writer never reuses memory it handed out: Data() stays valid until the
 * next write, which may reallocate.
 */
class COOKYOURGAME_API FCYGBitWriter
{
public:
	FCYGBitWriter() { Buffer.Reserve(256); }
	explicit FCYGBitWriter(int32 InitialCapacity) { Buffer.Reserve(FMath::Max(1, InitialCapacity)); }

	/** Forgets the content but keeps the allocation for the next packet. */
	void Reset() { Buffer.Reset(); }

	int32 Num() const { return Buffer.Num(); }
	const TArray<uint8>& Data() const { return Buffer; }

	void WriteUInt8(uint8 Value) { Buffer.Push(Value); }
	void WriteBool(bool bValue) { Buffer.Push(bValue ? 1 : 0); }

	/** Unsigned LEB128, 1..5 bytes. */
	void WriteUInt32(uint32 Value);
	/** Zig-zag LEB128, so small negative numbers stay small. */
	void WriteInt32(int32 Value);
	void WriteUInt64(uint64 Value);
	void WriteInt64(int64 Value);

	/** Four big-endian IEEE754 bytes; the wire is single precision even where the engine is not. */
	void WriteSingle(float Value);
	/** Two big-endian half-precision bytes. */
	void WriteHalf(float Value);

	/** Three big-endian singles: 12 bytes, no precision lost. */
	void WriteVector3(const FVector& Value);
	/**
	 * Three big-endian int16 fixed-point components, one hundredth of a world
	 * unit each. Senders working in centimetres should convert first, so the
	 * encoding keeps the same resolution as the rest of the room.
	 */
	void WriteVector3Packed(const FVector& Value);

	/** Four big-endian singles: 16 bytes. */
	void WriteQuaternionRaw(const FQuat& Value);
	/**
	 * Smallest-three packing in exactly four bytes: two bits name the component
	 * left out, the other three take ten bits each on a -1..1 range.
	 */
	void WriteQuaternion(const FQuat& Value);

	/** Big-endian fixed width, used for ids the server addresses by number. */
	void WriteUInt16BE(uint16 Value);
	void WriteUInt32BE(uint32 Value);

	/** UTF-8 bytes behind an LEB128 length. */
	void WriteString(const FString& Value);
	/** Raw bytes behind an LEB128 length. */
	void WriteBytes(const TArray<uint8>& Value);
	/** Appended verbatim, no length prefix. */
	void WriteRaw(const uint8* Value, int32 Count);
	void WriteRaw(const TArray<uint8>& Value) { WriteRaw(Value.GetData(), Value.Num()); }

private:
	TArray<uint8> Buffer;
};

/**
 * Cursor over one received packet. Reads past the end, or a varint that does
 * not terminate, latch an error instead of throwing; the caller checks
 * HasError() once after decoding a payload.
 */
class COOKYOURGAME_API FCYGBitReader
{
public:
	FCYGBitReader() = default;
	FCYGBitReader(const uint8* InData, int32 InLength) { Reset(InData, InLength); }

	void Reset(const uint8* InData, int32 InLength)
	{
		Data = InData;
		Length = (InData != nullptr) ? InLength : 0;
		Cursor = 0;
		bFailed = false;
	}

	bool HasError() const { return bFailed; }
	int32 Total() const { return Length; }
	int32 BytesRead() const { return Cursor; }
	int32 Remaining() const { return Length - Cursor; }

	uint8 ReadUInt8();
	bool ReadBool() { return ReadUInt8() != 0; }
	uint32 ReadUInt32();
	int32 ReadInt32();
	uint64 ReadUInt64();
	int64 ReadInt64();
	float ReadSingle();
	float ReadHalf();

	FVector ReadVector3();
	FVector ReadVector3Packed();
	FQuat ReadQuaternion();
	FQuat ReadQuaternionRaw();

	uint16 ReadUInt16BE();
	uint32 ReadUInt32BE();

	FString ReadString();
	TArray<uint8> ReadBytes();

	/** Moves past bytes this layer does not interpret, without copying them. */
	void Skip(int32 Count);
	/** Zero-copy view of what is left; valid while the source packet is. */
	const uint8* Remainder(int32& OutCount) const
	{
		OutCount = Length - Cursor;
		return Data != nullptr ? Data + Cursor : nullptr;
	}

	/** Reads a fixed number of bytes as text, without a length prefix. */
	FString ReadFixedText(int32 Count);
	/** Reads a fixed number of raw bytes. */
	TArray<uint8> ReadFixedBytes(int32 Count);

private:
	/** Consumes Count bytes, or fails the read without advancing past the end. */
	const uint8* Take(int32 Count);

	const uint8* Data = nullptr;
	int32 Length = 0;
	int32 Cursor = 0;
	bool bFailed = false;
};
