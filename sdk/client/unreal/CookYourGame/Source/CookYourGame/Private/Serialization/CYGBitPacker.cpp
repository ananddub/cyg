// Copyright CookYourGame. All rights reserved.

#include "Serialization/CYGBitPacker.h"

#include "Containers/StringConv.h"

namespace
{
	constexpr int32 FloatExponentBias = 127;
	constexpr int32 HalfExponentBias = 15;
	constexpr int32 QuaternionScale = 512;
	constexpr int32 QuaternionMask = (1 << 10) - 1;

	uint32 FloatToBits(float Value)
	{
		uint32 Bits = 0;
		FMemory::Memcpy(&Bits, &Value, sizeof(Bits));
		return Bits;
	}

	float BitsToFloat(uint32 Bits)
	{
		float Value = 0.f;
		FMemory::Memcpy(&Value, &Bits, sizeof(Value));
		return Value;
	}

	/** Ties break away from zero, so the same input encodes identically on every platform. */
	int32 RoundHalfAway(double Value)
	{
		return Value >= 0.0
			? static_cast<int32>(FMath::FloorToDouble(Value + 0.5))
			: -static_cast<int32>(FMath::FloorToDouble(-Value + 0.5));
	}

	uint16 FloatToHalf(float Value)
	{
		const uint32 Raw = FloatToBits(Value);
		const uint16 Sign = static_cast<uint16>((Raw >> 16) & 0x8000u);
		const uint32 BiasedExponent = (Raw >> 23) & 0xFFu;
		const uint32 Mantissa = Raw & 0x007FFFFFu;

		if (BiasedExponent == 0xFFu)
		{
			// Infinity keeps an empty payload; NaN always carries one so it stays NaN.
			return Sign | 0x7C00u | (Mantissa != 0u ? 0x0200u : 0u);
		}

		const int32 Exponent = static_cast<int32>(BiasedExponent) - FloatExponentBias + HalfExponentBias;

		if (Exponent >= 31)
		{
			return Sign | 0x7C00u;
		}

		if (Exponent <= 0)
		{
			if (Exponent < -10)
			{
				return Sign;
			}

			const uint32 WithImplicitBit = Mantissa | 0x00800000u;
			const int32 Shift = 14 - Exponent;
			uint32 Result = WithImplicitBit >> Shift;
			const uint32 Rest = WithImplicitBit & ((1u << Shift) - 1u);
			const uint32 Tie = 1u << (Shift - 1);

			if (Rest > Tie || (Rest == Tie && (Result & 1u) != 0u))
			{
				++Result;
			}

			return Sign | static_cast<uint16>(Result);
		}

		const uint32 HalfMantissa = Mantissa >> 13;
		uint32 Result = (static_cast<uint32>(Exponent) << 10) | HalfMantissa;
		const uint32 Rest = Mantissa & 0x1FFFu;

		if (Rest > 0x1000u || (Rest == 0x1000u && (HalfMantissa & 1u) != 0u))
		{
			++Result;
		}

		return Sign | static_cast<uint16>(Result);
	}

	float HalfToFloat(uint16 Value)
	{
		const uint32 Sign = static_cast<uint32>(Value & 0x8000) << 16;
		const int32 Exponent = (Value >> 10) & 0x1F;
		const uint32 Mantissa = Value & 0x3FFu;

		if (Exponent == 0)
		{
			if (Mantissa == 0)
			{
				return BitsToFloat(Sign);
			}

			// A subnormal half is mantissa * 2^-24; lift it to a normal exactly.
			int32 Highest = 0;
			for (uint32 Working = Mantissa; Working > 1u; Working >>= 1)
			{
				++Highest;
			}

			const uint32 NormalMantissa = (Mantissa - (1u << Highest)) << (23 - Highest);
			return BitsToFloat(Sign | (static_cast<uint32>(Highest + 103) << 23) | NormalMantissa);
		}

		if (Exponent == 31)
		{
			return BitsToFloat(Sign | 0x7F800000u | (Mantissa << 13));
		}

		return BitsToFloat(Sign | (static_cast<uint32>(Exponent + FloatExponentBias - HalfExponentBias) << 23) | (Mantissa << 13));
	}

	/** Maps -1..1 onto ten bits. */
	uint32 QuantizeUnit(float Value)
	{
		if (FMath::IsNaN(Value))
		{
			return QuaternionScale;
		}

		const int32 Encoded = RoundHalfAway(static_cast<double>(Value) * QuaternionScale) + QuaternionScale;
		return static_cast<uint32>(FMath::Clamp(Encoded, 0, QuaternionMask));
	}

	float UnquantizeUnit(uint32 Encoded)
	{
		return (static_cast<float>(Encoded) - QuaternionScale) / QuaternionScale;
	}

	/** Fixed-point axis at one hundredth of a world unit. */
	uint16 QuantizeAxis(float Value)
	{
		int32 Scaled = 0;

		if (!FMath::IsNaN(Value) && FMath::IsFinite(Value))
		{
			Scaled = RoundHalfAway(static_cast<double>(Value) * 100.0);
			Scaled = FMath::Clamp(Scaled, -32768, 32767);
		}

		return static_cast<uint16>(Scaled);
	}

	float DequantizeAxis(uint16 Value)
	{
		return static_cast<int16>(Value) / 100.0f;
	}
}

void FCYGBitWriter::WriteUInt32(uint32 Value)
{
	while (Value >= 0x80u)
	{
		Buffer.Push(static_cast<uint8>(Value) | 0x80u);
		Value >>= 7;
	}

	Buffer.Push(static_cast<uint8>(Value));
}

void FCYGBitWriter::WriteInt32(int32 Value)
{
	WriteUInt32((static_cast<uint32>(Value) << 1) ^ static_cast<uint32>(Value >> 31));
}

void FCYGBitWriter::WriteUInt64(uint64 Value)
{
	while (Value >= 0x80ull)
	{
		Buffer.Push(static_cast<uint8>(Value) | 0x80ull);
		Value >>= 7;
	}

	Buffer.Push(static_cast<uint8>(Value));
}

void FCYGBitWriter::WriteInt64(int64 Value)
{
	WriteUInt64((static_cast<uint64>(Value) << 1) ^ static_cast<uint64>(Value >> 63));
}

void FCYGBitWriter::WriteSingle(float Value)
{
	WriteUInt32BE(FloatToBits(Value));
}

void FCYGBitWriter::WriteHalf(float Value)
{
	WriteUInt16BE(FloatToHalf(Value));
}

void FCYGBitWriter::WriteUInt16BE(uint16 Value)
{
	Buffer.Push(static_cast<uint8>(Value >> 8));
	Buffer.Push(static_cast<uint8>(Value));
}

void FCYGBitWriter::WriteUInt32BE(uint32 Value)
{
	Buffer.Push(static_cast<uint8>(Value >> 24));
	Buffer.Push(static_cast<uint8>(Value >> 16));
	Buffer.Push(static_cast<uint8>(Value >> 8));
	Buffer.Push(static_cast<uint8>(Value));
}

void FCYGBitWriter::WriteVector3(const FVector& Value)
{
	WriteSingle(static_cast<float>(Value.X));
	WriteSingle(static_cast<float>(Value.Y));
	WriteSingle(static_cast<float>(Value.Z));
}

void FCYGBitWriter::WriteVector3Packed(const FVector& Value)
{
	WriteUInt16BE(QuantizeAxis(static_cast<float>(Value.X)));
	WriteUInt16BE(QuantizeAxis(static_cast<float>(Value.Y)));
	WriteUInt16BE(QuantizeAxis(static_cast<float>(Value.Z)));
}

void FCYGBitWriter::WriteQuaternionRaw(const FQuat& Value)
{
	WriteSingle(static_cast<float>(Value.X));
	WriteSingle(static_cast<float>(Value.Y));
	WriteSingle(static_cast<float>(Value.Z));
	WriteSingle(static_cast<float>(Value.W));
}

void FCYGBitWriter::WriteQuaternion(const FQuat& Value)
{
	float Components[4] =
	{
		static_cast<float>(Value.X),
		static_cast<float>(Value.Y),
		static_cast<float>(Value.Z),
		static_cast<float>(Value.W),
	};

	const float LengthSquared =
		Components[0] * Components[0] +
		Components[1] * Components[1] +
		Components[2] * Components[2] +
		Components[3] * Components[3];

	if (LengthSquared > 0.f && FMath::IsFinite(LengthSquared))
	{
		const float InverseLength = FMath::InvSqrt(LengthSquared);
		Components[0] *= InverseLength;
		Components[1] *= InverseLength;
		Components[2] *= InverseLength;
		Components[3] *= InverseLength;
	}

	int32 Largest = 0;
	float Magnitude = FMath::Abs(Components[0]);

	for (int32 Index = 1; Index < 4; ++Index)
	{
		const float Candidate = FMath::Abs(Components[Index]);
		if (Candidate > Magnitude)
		{
			Largest = Index;
			Magnitude = Candidate;
		}
	}

	uint32 Packed = static_cast<uint32>(Largest) << 30;
	int32 Slot = 0;

	for (int32 Index = 0; Index < 4; ++Index)
	{
		if (Index == Largest)
		{
			continue;
		}

		Packed |= QuantizeUnit(Components[Index]) << (20 - Slot * 10);
		++Slot;
	}

	WriteUInt32BE(Packed);
}

void FCYGBitWriter::WriteString(const FString& Value)
{
	if (Value.IsEmpty())
	{
		WriteUInt32(0);
		return;
	}

	const FTCHARToUTF8 Convert(*Value);
	WriteUInt32(static_cast<uint32>(Convert.Length()));
	WriteRaw(reinterpret_cast<const uint8*>(Convert.Get()), Convert.Length());
}

void FCYGBitWriter::WriteBytes(const TArray<uint8>& Value)
{
	WriteUInt32(static_cast<uint32>(Value.Num()));
	WriteRaw(Value.GetData(), Value.Num());
}

void FCYGBitWriter::WriteRaw(const uint8* Value, int32 Count)
{
	if (Value == nullptr || Count <= 0)
	{
		return;
	}

	Buffer.Append(Value, Count);
}

const uint8* FCYGBitReader::Take(int32 Count)
{
	if (Count < 0 || Cursor + Count > Length)
	{
		bFailed = true;
		Cursor = Length;
		return nullptr;
	}

	const uint8* Found = Data + Cursor;
	Cursor += Count;
	return Found;
}

uint8 FCYGBitReader::ReadUInt8()
{
	const uint8* Bytes = Take(1);
	return Bytes != nullptr ? *Bytes : 0;
}

uint32 FCYGBitReader::ReadUInt32()
{
	uint32 Result = 0;

	for (int32 Shift = 0; Shift <= 28; Shift += 7)
	{
		const uint8* Byte = Take(1);
		if (Byte == nullptr)
		{
			return 0;
		}

		Result |= static_cast<uint32>(*Byte & 0x7F) << Shift;

		if ((*Byte & 0x80) == 0)
		{
			return Result;
		}
	}

	bFailed = true;
	return 0;
}

int32 FCYGBitReader::ReadInt32()
{
	const uint32 Raw = ReadUInt32();
	return static_cast<int32>(Raw >> 1) ^ -static_cast<int32>(Raw & 1u);
}

uint64 FCYGBitReader::ReadUInt64()
{
	uint64 Result = 0;

	for (int32 Shift = 0; Shift <= 63; Shift += 7)
	{
		const uint8* Byte = Take(1);
		if (Byte == nullptr)
		{
			return 0;
		}

		Result |= static_cast<uint64>(*Byte & 0x7F) << Shift;

		if ((*Byte & 0x80) == 0)
		{
			return Result;
		}
	}

	bFailed = true;
	return 0;
}

int64 FCYGBitReader::ReadInt64()
{
	const uint64 Raw = ReadUInt64();
	return static_cast<int64>(Raw >> 1) ^ -static_cast<int64>(Raw & 1ull);
}

float FCYGBitReader::ReadSingle()
{
	const uint8* Bytes = Take(4);
	if (Bytes == nullptr)
	{
		return 0.f;
	}

	const uint32 Raw =
		(static_cast<uint32>(Bytes[0]) << 24) |
		(static_cast<uint32>(Bytes[1]) << 16) |
		(static_cast<uint32>(Bytes[2]) << 8) |
		static_cast<uint32>(Bytes[3]);

	return BitsToFloat(Raw);
}

float FCYGBitReader::ReadHalf()
{
	return HalfToFloat(ReadUInt16BE());
}

uint16 FCYGBitReader::ReadUInt16BE()
{
	const uint8* Bytes = Take(2);
	if (Bytes == nullptr)
	{
		return 0;
	}

	return static_cast<uint16>((Bytes[0] << 8) | Bytes[1]);
}

uint32 FCYGBitReader::ReadUInt32BE()
{
	const uint8* Bytes = Take(4);
	if (Bytes == nullptr)
	{
		return 0;
	}

	return
		(static_cast<uint32>(Bytes[0]) << 24) |
		(static_cast<uint32>(Bytes[1]) << 16) |
		(static_cast<uint32>(Bytes[2]) << 8) |
		static_cast<uint32>(Bytes[3]);
}

FVector FCYGBitReader::ReadVector3()
{
	const float X = ReadSingle();
	const float Y = ReadSingle();
	const float Z = ReadSingle();
	return FVector(X, Y, Z);
}

FVector FCYGBitReader::ReadVector3Packed()
{
	const uint16 X = ReadUInt16BE();
	const uint16 Y = ReadUInt16BE();
	const uint16 Z = ReadUInt16BE();
	return FVector(DequantizeAxis(X), DequantizeAxis(Y), DequantizeAxis(Z));
}

FQuat FCYGBitReader::ReadQuaternion()
{
	const uint32 Packed = ReadUInt32BE();
	if (bFailed)
	{
		return FQuat::Identity;
	}

	const int32 Largest = static_cast<int32>((Packed >> 30) & 0x3u);
	const float First = UnquantizeUnit((Packed >> 20) & static_cast<uint32>(QuaternionMask));
	const float Second = UnquantizeUnit((Packed >> 10) & static_cast<uint32>(QuaternionMask));
	const float Third = UnquantizeUnit(Packed & static_cast<uint32>(QuaternionMask));

	// The dropped component comes back from the unit constraint with a positive sign:
	// a rotation and its negation describe the same orientation.
	const float Sum = First * First + Second * Second + Third * Third;
	const float Remaining = Sum < 1.f ? FMath::Sqrt(1.f - Sum) : 0.f;

	// Largest component was W, so the three decoded values are already X, Y and Z.
	switch (Largest)
	{
	case 0: return FQuat(Remaining, First, Second, Third);
	case 1: return FQuat(First, Remaining, Second, Third);
	case 2: return FQuat(First, Second, Remaining, Third);
	default: return FQuat(First, Second, Third, Remaining);
	}
}

FQuat FCYGBitReader::ReadQuaternionRaw()
{
	const float X = ReadSingle();
	const float Y = ReadSingle();
	const float Z = ReadSingle();
	const float W = ReadSingle();
	return FQuat(X, Y, Z, W);
}

FString FCYGBitReader::ReadString()
{
	const uint32 Count = ReadUInt32();
	if (bFailed || Count == 0)
	{
		return FString();
	}

	if (static_cast<int64>(Cursor) + static_cast<int64>(Count) > Length)
	{
		bFailed = true;
		Cursor = Length;
		return FString();
	}

	const FUTF8ToTCHAR Convert(reinterpret_cast<const ANSICHAR*>(Data + Cursor), static_cast<int32>(Count));
	Cursor += static_cast<int32>(Count);

	// The helper null-terminates its output, so the pointer forms a valid string.
	return FString(Convert.Get());
}

TArray<uint8> FCYGBitReader::ReadBytes()
{
	TArray<uint8> Result;
	const uint32 Count = ReadUInt32();

	if (bFailed || Count == 0)
	{
		return Result;
	}

	if (static_cast<int64>(Cursor) + static_cast<int64>(Count) > Length)
	{
		bFailed = true;
		Cursor = Length;
		return Result;
	}

	Result.Append(Data + Cursor, static_cast<int32>(Count));
	Cursor += static_cast<int32>(Count);
	return Result;
}

void FCYGBitReader::Skip(int32 Count)
{
	Take(Count);
}

FString FCYGBitReader::ReadFixedText(int32 Count)
{
	const uint8* Bytes = Take(Count);

	if (Bytes == nullptr || Count <= 0)
	{
		return FString();
	}

	return FString(FUTF8ToTCHAR(reinterpret_cast<const ANSICHAR*>(Bytes), Count).Get());
}

TArray<uint8> FCYGBitReader::ReadFixedBytes(int32 Count)
{
	TArray<uint8> Result;
	const uint8* Bytes = Take(Count);

	if (Bytes != nullptr && Count > 0)
	{
		Result.Append(Bytes, Count);
	}

	return Result;
}
