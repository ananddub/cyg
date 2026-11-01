// Delta row decoder — the client half of commitState().
//
// Wire (server commitState → TLV rows):
//   [tag u8][len u8][id u64 LE][len bytes of fields]
//   - entity rows:   tags 0x01.. in collection declaration order; id = player/entity id;
//     payload = fields concatenated in declaration order (t.number = f64 LE, t.int32 = i32 LE …)
//   - root scalars:  one row tagged 0x10 (values in declaration order)
// Field order matches `ox-game schema-codegen --lang cs` output
// (CYGSchemaGenerated.cs), so decoded columns map straight onto those classes.

using System;
using System.Collections.Generic;

namespace CYG
{
    public struct CygField
    {
        public string Name;
        public double Number;      // t.number
        public int Int;            // t.int32
        public long Long;          // t.int64
    }

    public struct CygRow
    {
        public byte Tag;
        public bool IsRoot;        // 0x10 = root scalars
        public ulong EntityId;     // entity rows
        public CygField[] Fields;
    }

    public static class CygDeltaDecoder
    {
        /// Parse one delta packet into rows. Field names come from the caller's
        /// schema map (tag → field names, taken from CYGSchemaGenerated / descriptor).
        public static List<CygRow> Decode(byte[] delta, IReadOnlyDictionary<byte, string[]> tagFields)
        {
            var rows = new List<CygRow>();
            int i = 0;
            // Header forms — the server's wrap_delta_into emits
            // [0x11 proto][0xA5]['d','e','l','t','a'][your_id i64] = 15 bytes,
            // and bare [0xA5]['d','e','l','t','a'][id i64] = 14 bytes. The id
            // is per-row inside the TLV records too, so the envelope's copy
            // is stripped with the magic. (The old 7/6-byte strip left the id
            // in place and every row decoded one id-width misaligned.)
            if (delta.Length >= 15 && delta[0] == 0x11 && delta[1] == 0xA5
                && delta[2] == (byte)'d' && delta[3] == (byte)'e'
                && delta[4] == (byte)'l' && delta[5] == (byte)'t'
                && delta[6] == (byte)'a') i = 15;
            else if (delta.Length >= 14 && delta[0] == 0xA5 && delta[1] == 0x64
                && delta[2] == (byte)'e' && delta[3] == (byte)'l'
                && delta[4] == (byte)'t' && delta[5] == (byte)'a') i = 14;

            while (i + 10 <= delta.Length)
            {
                byte tag = delta[i];
                int len = delta[i + 1];
                if (len == 0 || i + 10 + len > delta.Length) break;
                ulong id = 0;
                for (int b = 0; b < 8; b++) id |= (ulong)delta[i + 2 + b] << (8 * b);
                int payload = i + 10;
                i += 10 + len;

                tagFields.TryGetValue(tag, out var names);
                var fields = DecodeFields(delta, payload, len, names);
                rows.Add(new CygRow { Tag = tag, IsRoot = tag == 0x10, EntityId = id, Fields = fields });
            }
            return rows;
        }

        private static CygField[] DecodeFields(byte[] delta, int start, int len, string[] names)
        {
            var fields = new List<CygField>();
            int i = start, end = start + len, nameIndex = 0;
            while (i + 8 <= end)
            {
                var f = new CygField { Number = BitConverter.ToDouble(delta, i) };
                if (names != null && nameIndex < names.Length) f.Name = names[nameIndex];
                fields.Add(f);
                nameIndex++;
                i += 8;
            }
            return fields.ToArray();
        }
    }
}
