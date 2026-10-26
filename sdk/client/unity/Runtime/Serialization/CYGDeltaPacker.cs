using System;
using System.Text;
using UnityEngine;

namespace CookYourGame
{
    /// <summary>
    /// Binary delta diff/patch for the CYG networking SDK: a self-contained implementation of the
    /// public-domain Fossil/SQLite delta idea. CreateDelta encodes the difference between a shared
    /// base byte[] and a target byte[]; ApplyDelta replays it to rebuild the target exactly.
    /// NOT reentrant: it uses static scratch buffers guarded by an "encoder busy" flag, so the
    /// caller must invoke it from a single thread (acceptable for the Unity main thread).
    ///
    /// WIRE LAYOUT (byte-level; all varints are LEB128, 7 data bits + high continuation bit, LSB
    /// first -- identical to CYGBitWriter.WriteUInt32; all "big-endian" fields are most-significant
    /// byte first):
    ///   header  = varint baseSize, varint targetSize
    ///     baseSize    = length of the base array this delta was made against (informational/validation)
    ///     targetSize  = byte length of the reconstructed target (used to validate and bound output)
    ///   records = zero or more records, each starting with one control byte c:
    ///     ADD   (c &amp; 0x80 != 0): count = c &amp; 0x7F (1..127); the next count bytes are copied literally.
    ///                             Literal runs longer than 127 are split into consecutive ADD records.
    ///     COPY  (c &amp; 0x80 == 0): mode = c &gt;&gt; 5; n = c &amp; 31.
    ///                             mode 0..2  -&gt; length = mode + 2  (short copies of 2..4 bytes, no length bytes).
    ///                             mode 3     -&gt; the next 3 bytes are a big-endian length (5..2^24-1);
    ///                                           a run of exactly 4 is emitted as length 3 + length 2.
    ///                             Then n bytes follow, a big-endian offset into the base (n = 0 means offset 0),
    ///                                           so offsets reach 2^248. The decoder appends
    ///                                           base[offset .. offset+length-1]; offset+length must be &lt;= baseSize.
    ///   An empty target (targetSize 0) is encoded as the header alone (zero records).
    ///   CreateDelta guarantees the emitted bytes never exceed [header + one whole-target ADD] + slack,
    ///   so it always falls back to an all-ADD encoding rather than emitting a delta larger than a copy.
    /// </summary>
    public static class CYGDeltaPacker
    {
        const int ZipSize = 16;
        const int HashSize = 4096;
        const int MaxSearchWindow = 0x400000;
        const int MaxBaseSize = 0x10000000;
        const int MaxTargetSize = 0x40000000;
        const int LongMode = 3;
        const int MaxAddChunk = 127;
        const string Err = "[CYG] CYGDeltaPacker: ";

        static readonly int[] hashTable = new int[HashSize];
        static readonly int[] hashSum = new int[HashSize];
        static readonly int[] hashWeighted = new int[HashSize];
        static int scanLimit = HashSize;
        static bool busy;

        /// <summary>Computes a compact Fossil-style delta that transforms baseData into targetData; always succeeds.</summary>
        public static byte[] CreateDelta(byte[] baseData, byte[] targetData)
        {
            if (targetData == null)
            {
                targetData = Array.Empty<byte>();
            }

            int targetSize = targetData.Length;
            if (baseData == null)
            {
                baseData = Array.Empty<byte>();
            }

            int baseSize = baseData.Length;
            int hashBound = 0;
            bool hashed = false;

            if (baseSize >= ZipSize && baseSize <= MaxBaseSize)
            {
                hashBound = baseSize - ZipSize + 1;
                if (hashBound > MaxSearchWindow)
                {
                    hashBound = MaxSearchWindow;
                }

                hashed = TryBeginEncode();
                if (hashed)
                {
                    try
                    {
                        BuildHashTable(baseData, hashBound);
                        byte[] delta = Encode(baseData, hashBound, targetData, targetSize);
                        if (delta != null)
                        {
                            return delta;
                        }
                    }
                    finally
                    {
                        EndEncode();
                    }
                }
            }

            return BuildLiteralDelta(baseSize, targetData, targetSize);
        }

        /// <summary>Replays a delta produced by CreateDelta; throws InvalidOperationException on malformed input.</summary>
        public static byte[] ApplyDelta(byte[] baseData, byte[] delta)
        {
            byte[] target;
            if (!TryApplyDelta(baseData, delta, out target))
            {
                throw new InvalidOperationException(Err + "delta is malformed or does not match the base.");
            }

            return target;
        }

        /// <summary>Non-throwing, bounds-checked ApplyDelta; returns false instead of throwing on any malformed input.</summary>
        public static bool TryApplyDelta(byte[] baseData, byte[] delta, out byte[] target)
        {
            target = null;
            if (delta == null)
            {
                Debug.LogWarning(Err + "ApplyDelta received a null delta.");
                return false;
            }

            if (baseData == null)
            {
                baseData = Array.Empty<byte>();
            }

            int baseSize = baseData.Length;
            int cursor = 0;

            uint declaredBase;
            if (!ReadVarint(delta, ref cursor, out declaredBase))
            {
                Debug.LogWarning(Err + "truncated base-size varint in header.");
                return false;
            }

            uint declaredTarget;
            if (!ReadVarint(delta, ref cursor, out declaredTarget))
            {
                Debug.LogWarning(Err + "truncated target-size varint in header.");
                return false;
            }

            if (declaredTarget > (uint)MaxTargetSize)
            {
                Debug.LogWarning(Err + "declared target size exceeds the guard.");
                return false;
            }

            int targetSize = (int)declaredTarget;
            byte[] output = new byte[targetSize];
            int produced = 0;

            while (cursor < delta.Length)
            {
                byte control = delta[cursor++];
                if ((control & 0x80) != 0)
                {
                    int count = control & 0x7F;
                    if (count == 0 || count > delta.Length - cursor || count > targetSize - produced)
                    {
                        Debug.LogWarning(Err + "ADD record reads past the delta or overflows the target.");
                        return false;
                    }

                    Buffer.BlockCopy(delta, cursor, output, produced, count);
                    cursor += count;
                    produced += count;
                }
                else
                {
                    int mode = control >> 5;
                    int n = control & 31;
                    int length;
                    if (mode == LongMode)
                    {
                        if (cursor + 3 > delta.Length)
                        {
                            Debug.LogWarning(Err + "truncated long copy length.");
                            return false;
                        }

                        length = (delta[cursor] << 16) | (delta[cursor + 1] << 8) | delta[cursor + 2];
                        cursor += 3;
                        if (length < 5)
                        {
                            Debug.LogWarning(Err + "long copy length is out of range.");
                            return false;
                        }
                    }
                    else
                    {
                        length = mode + 2;
                    }

                    if (n > delta.Length - cursor)
                    {
                        Debug.LogWarning(Err + "truncated copy offset.");
                        return false;
                    }

                    long offset = 0;
                    for (int i = 0; i < n; i++)
                    {
                        offset = (offset << 8) | delta[cursor++];
                    }

                    if (offset < 0 || offset > baseSize || length > baseSize - offset)
                    {
                        Debug.LogWarning(Err + "copy window lies outside the base array.");
                        return false;
                    }

                    if (length > targetSize - produced)
                    {
                        Debug.LogWarning(Err + "copy overflows the declared target size.");
                        return false;
                    }

                    Buffer.BlockCopy(baseData, (int)offset, output, produced, length);
                    produced += length;
                }
            }

            if (produced != targetSize)
            {
                Debug.LogWarning(Err + "produced " + produced + " bytes but the header declared " + targetSize + ".");
                return false;
            }

            target = output;
            return true;
        }

        /// <summary>Exact encoded byte count for a given delta (header + records), used to decide copy vs literal.</summary>
        public static int EncodedSize(int baseSize, int targetSize, int recordCount)
        {
            int size = VarintSize((uint)baseSize) + VarintSize((uint)targetSize);
            if (targetSize > 0)
            {
                int fullAdds = targetSize / MaxAddChunk;
                int tail = targetSize % MaxAddChunk;
                size += fullAdds * (1 + MaxAddChunk);
                if (tail != 0)
                {
                    size += 1 + tail;
                }
            }

            if (recordCount > 0)
            {
                size += recordCount * 3;
            }

            return size;
        }

        /// <summary>Round-trips a spread of edge and randomized cases so a developer can sanity-check the codec from a menu.</summary>
        public static bool RunSelfTest(StringBuilder log)
        {
            bool allPassed = true;
            int cases = 0;

            byte[] empty = new byte[0];
            byte[] small = new byte[] { 1, 2, 3, 2, 1, 4, 4, 4, 1, 2, 3, 9, 9 };

            allPassed &= CheckCase("empty vs empty", empty, empty, ref cases, log);
            allPassed &= CheckCase("null-ish base literal", empty, small, ref cases, log);
            allPassed &= CheckCase("identical", small, small, ref cases, log);
            allPassed &= CheckCase("base longer than target", small, new byte[] { 1, 2, 3 }, ref cases, log);
            allPassed &= CheckCase("no common bytes", new byte[] { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 },
                new byte[] { 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7 }, ref cases, log);

            byte[] baseBig = Ramp(4096);
            byte[] targetEdited = (byte[])baseBig.Clone();
            targetEdited[10] = 0xEE;
            targetEdited[1000] = 0xFF;
            targetEdited[3000] = 0x11;
            allPassed &= CheckCase("edited snapshot", baseBig, targetEdited, ref cases, log);

            byte[] targetShifted = new byte[4106];
            Buffer.BlockCopy(baseBig, 0, targetShifted, 10, 4096);
            allPassed &= CheckCase("shifted snapshot", baseBig, targetShifted, ref cases, log);

            System.Random random = new System.Random(12345);
            for (int i = 0; i < 200; i++)
            {
                byte[] a = RandomBytes(random, random.Next(0, 300));
                byte[] b = RandomBytes(random, random.Next(0, 300));
                allPassed &= CheckCase("fuzz#" + i, a, b, ref cases, log);
            }

            if (log != null)
            {
                log.Append("CYGDeltaPacker self-test: ").Append(cases).Append(" cases, ")
                   .Append(allPassed ? "ALL PASSED" : "FAILURES PRESENT").Append('.');
            }

            return allPassed;
        }

        static byte[] Encode(byte[] baseData, int hashBound, byte[] target, int targetSize)
        {
            byte[] outBuf = new byte[targetSize + targetSize / 4 + 64];
            int outPos = 0;

            outPos = WriteVarintInto(outBuf, outPos, (uint)baseData.Length);
            outPos = WriteVarintInto(outBuf, outPos, (uint)targetSize);

            if (targetSize == 0)
            {
                byte[] emptyDelta = new byte[outPos];
                Buffer.BlockCopy(outBuf, 0, emptyDelta, 0, outPos);
                return emptyDelta;
            }

            int fullAddSize = targetSize + VarintSize((uint)baseData.Length) + VarintSize((uint)targetSize) + 16;
            int i = 0;
            int literalStart = 0;
            bool rollingValid = false;
            int windowSum = 0;
            int windowWeighted = 0;
            bool failed = false;

            while (i + ZipSize <= targetSize)
            {
                if (!rollingValid)
                {
                    InitWindow(target, i, out windowSum, out windowWeighted);
                    rollingValid = true;
                }
                else
                {
                    int outByte = target[i - 1];
                    int inByte = target[i + ZipSize - 1];
                    windowWeighted -= windowSum;
                    windowSum += inByte - outByte;
                    windowWeighted += ZipSize * inByte;
                }

                int hash = CombineHash(windowSum, windowWeighted);
                int candidate = FindMatch(baseData, hash, target, i, ZipSize, hashBound);

                if (candidate >= 0)
                {
                    int matchLen = ZipSize;
                    while (i + matchLen < targetSize && candidate + matchLen < baseData.Length &&
                           baseData[candidate + matchLen] == target[i + matchLen])
                    {
                        matchLen++;
                    }

                    if (FlushLiteral(outBuf, ref outPos, target, literalStart, i) < 0)
                    {
                        failed = true;
                        break;
                    }

                    int written = WriteCopyRecord(outBuf, outPos, candidate, matchLen);
                    if (written < 0)
                    {
                        failed = true;
                        break;
                    }

                    outPos += written;
                    i += matchLen;
                    literalStart = i;
                    rollingValid = false;
                }
                else
                {
                    i++;
                }
            }

            if (!failed && FlushLiteral(outBuf, ref outPos, target, literalStart, targetSize) < 0)
            {
                failed = true;
            }

            if (failed || outPos >= fullAddSize)
            {
                return null;
            }

            byte[] delta = new byte[outPos];
            Buffer.BlockCopy(outBuf, 0, delta, 0, outPos);
            return delta;
        }

        static void BuildHashTable(byte[] baseData, int hashBound)
        {
            for (int k = 0; k < HashSize; k++)
            {
                hashTable[k] = -1;
            }

            int sum = 0;
            int weighted = 0;
            int first = ZipSize < hashBound ? ZipSize : hashBound;
            for (int j = 0; j < first; j++)
            {
                sum += baseData[j];
                weighted += (j + 1) * baseData[j];
            }

            int hash = CombineHash(sum, weighted);
            Insert(hash, 0, sum, weighted);

            for (int i = 1; i < hashBound; i++)
            {
                int outByte = baseData[i - 1];
                int inByte = baseData[i + ZipSize - 1];
                weighted -= sum;
                sum += inByte - outByte;
                weighted += ZipSize * inByte;
                hash = CombineHash(sum, weighted);
                Insert(hash, i, sum, weighted);
            }

            scanLimit = hashBound < HashSize ? hashBound : HashSize;
        }

        static void Insert(int hash, int offset, int sum, int weighted)
        {
            // Empty slots are marked hashTable[hash] = -1. Refresh a slot only when this window's
            // checksum differs, so a chain of identical windows keeps its earliest offset and a
            // legitimate all-zero window (checksum 0) is still recorded.
            if (hashTable[hash] >= 0 && hashSum[hash] == sum && hashWeighted[hash] == weighted)
            {
                return;
            }

            hashSum[hash] = sum;
            hashWeighted[hash] = weighted;
            hashTable[hash] = offset;
        }

        static int FindMatch(byte[] baseData, int hash, byte[] target, int position, int length, int hashBound)
        {
            int candidate = hashTable[hash];
            int checkedCount = 0;
            while (candidate >= 0 && checkedCount < scanLimit)
            {
                if (candidate < hashBound && RegionEquals(baseData, candidate, target, position, length))
                {
                    return candidate;
                }

                checkedCount++;
                candidate = -1;
            }

            return -1;
        }

        static bool RegionEquals(byte[] left, int leftStart, byte[] right, int rightStart, int count)
        {
            for (int k = 0; k < count; k++)
            {
                if (left[leftStart + k] != right[rightStart + k])
                {
                    return false;
                }
            }

            return true;
        }

        static int CombineHash(int sum, int weighted)
        {
            unchecked
            {
                uint h = (uint)sum * 2654435761u + (uint)weighted * 40503u;
                return (int)(h & (uint)(HashSize - 1));
            }
        }

        static void InitWindow(byte[] data, int at, out int sum, out int weighted)
        {
            int s = 0;
            int w = 0;
            for (int j = 0; j < ZipSize; j++)
            {
                byte b = data[at + j];
                s += b;
                w += (j + 1) * b;
            }

            sum = s;
            weighted = w;
        }

        static int FlushLiteral(byte[] outBuf, ref int outPos, byte[] target, int from, int to)
        {
            int count = to - from;
            while (count > 0)
            {
                int chunk = count > MaxAddChunk ? MaxAddChunk : count;
                if (chunk == 1)
                {
                    if (outPos + 2 > outBuf.Length)
                    {
                        return -1;
                    }

                    outBuf[outPos++] = (byte)(0x80 | 1);
                    outBuf[outPos++] = target[from];
                    from++;
                }
                else
                {
                    if (outPos + 1 + chunk > outBuf.Length)
                    {
                        return -1;
                    }

                    outBuf[outPos++] = (byte)(0x80 | chunk);
                    Buffer.BlockCopy(target, from, outBuf, outPos, chunk);
                    outPos += chunk;
                    from += chunk;
                }

                count = to - from;
            }

            return 0;
        }

        // Writes one COPY record starting at 'at'; returns the number of bytes emitted, or -1 if it does not fit.
        static int WriteCopyRecord(byte[] outBuf, int at, int offset, int length)
        {
            int n = 0;
            while ((offset >> (8 * n)) != 0 && n < 4)
            {
                n++;
            }

            bool longForm = length >= 5;
            int control = longForm ? (LongMode << 5) : ((length - 2) << 5);
            control |= n;

            int total = 1 + (longForm ? 3 : 0) + n;
            if (at + total > outBuf.Length)
            {
                return -1;
            }

            int cursor = at;
            outBuf[cursor++] = (byte)control;
            if (longForm)
            {
                outBuf[cursor++] = (byte)(length >> 16);
                outBuf[cursor++] = (byte)(length >> 8);
                outBuf[cursor++] = (byte)length;
            }

            for (int shift = (n - 1) * 8; shift >= 0; shift -= 8)
            {
                outBuf[cursor++] = (byte)(offset >> shift);
            }

            return total;
        }

        static byte[] BuildLiteralDelta(int baseSize, byte[] target, int targetSize)
        {
            byte[] outBuf = new byte[targetSize + targetSize / 4 + 64];
            int outPos = 0;
            outPos = WriteVarintInto(outBuf, outPos, (uint)baseSize);
            outPos = WriteVarintInto(outBuf, outPos, (uint)targetSize);

            int from = 0;
            while (from < targetSize)
            {
                int chunk = targetSize - from;
                if (chunk > MaxAddChunk)
                {
                    chunk = MaxAddChunk;
                }

                if (chunk == 1)
                {
                    outBuf[outPos++] = (byte)(0x80 | 1);
                    outBuf[outPos++] = target[from];
                    from++;
                }
                else
                {
                    outBuf[outPos++] = (byte)(0x80 | chunk);
                    Buffer.BlockCopy(target, from, outBuf, outPos, chunk);
                    outPos += chunk;
                    from += chunk;
                }
            }

            byte[] delta = new byte[outPos];
            Buffer.BlockCopy(outBuf, 0, delta, 0, outPos);
            return delta;
        }

        static bool TryBeginEncode()
        {
            if (busy)
            {
                return false;
            }

            busy = true;
            return true;
        }

        static void EndEncode()
        {
            busy = false;
        }

        static int VarintSize(uint value)
        {
            int count = 1;
            while (value >= 0x80u)
            {
                value >>= 7;
                count++;
            }

            return count;
        }

        static int WriteVarintInto(byte[] buffer, int at, uint value)
        {
            while (value >= 0x80u)
            {
                buffer[at++] = (byte)(value | 0x80u);
                value >>= 7;
            }

            buffer[at++] = (byte)value;
            return at;
        }

        static bool ReadVarint(byte[] buffer, ref int index, out uint value)
        {
            value = 0;
            int shift = 0;
            while (true)
            {
                if (index >= buffer.Length)
                {
                    return false;
                }

                byte current = buffer[index++];
                value |= (uint)(current & 0x7F) << shift;
                if ((current & 0x80) == 0)
                {
                    return true;
                }

                shift += 7;
                if (shift > 31)
                {
                    return false;
                }
            }
        }

        static byte[] Ramp(int length)
        {
            byte[] data = new byte[length];
            for (int i = 0; i < length; i++)
            {
                data[i] = (byte)(i & 0xFF);
            }

            return data;
        }

        static byte[] RandomBytes(System.Random random, int length)
        {
            byte[] data = new byte[length];
            for (int i = 0; i < length; i++)
            {
                data[i] = (byte)random.Next(0, 4);
            }

            return data;
        }

        static bool CheckCase(string name, byte[] baseData, byte[] target, ref int cases, StringBuilder log)
        {
            cases++;
            byte[] delta = CreateDelta(baseData, target);
            byte[] rebuilt;
            bool ok = TryApplyDelta(baseData, delta, out rebuilt) && BytesEqual(rebuilt, target);
            if (!ok && log != null)
            {
                log.Append("FAIL: ").Append(name).AppendLine();
            }

            return ok;
        }

        static bool BytesEqual(byte[] a, byte[] b)
        {
            if (a == null || b == null)
            {
                return false;
            }

            if (a.Length != b.Length)
            {
                return false;
            }

            for (int i = 0; i < a.Length; i++)
            {
                if (a[i] != b[i])
                {
                    return false;
                }
            }

            return true;
        }
    }
}
