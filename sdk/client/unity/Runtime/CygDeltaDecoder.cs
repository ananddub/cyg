// Delta rows — a THIN SHIM over the cyg-core-rs native lib.
//
// The old TLV/OXS2 decoder that lived in this file (envelope strip, per-tag
// rows, f64 column splitting) has been removed: ALL schema decode now happens
// in cyg-core-rs (crates/cyg-core-rs), the same core the web (wasm), Godot
// (GDExtension) and Unreal SDKs use. CygClient builds a CygRoom from the
// host's "OXSD" descriptor frame and feeds every schema delta through it;
// this shim only re-shapes the core's decoded state into the legacy row form
// for callers (and generated code) that still expects CygRow lists.
//
// Wire notes kept for history: the server's schema delta travels as
// [0x11][0xA5 "delta"]… or [0xA5 "delta"]… — the core unwraps both.

using System;
using System.Collections.Generic;

namespace CYG
{
    public struct CygField
    {
        public string Name;
        public double Number;      // numeric fields (f64 view; i64 loses precision)
        public long Long;          // exact i64
        public string Str;         // string fields
        public bool? Bool;         // bool fields
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
        /// The decoded room (set by <see cref="Attach"/>, called by CygClient
        /// when the descriptor lands — or manually in tests).
        private static Cyg.CygRoom _room;
        /// Field names used for entity rows, indexed by schema field position.
        private static string[] _entityFields;

        /// Route decode through `room`. `entityFields` names the entity schema's
        /// fields in schema order (omit to use the core's field lookup).
        public static void Attach(Cyg.CygRoom room, string[] entityFields = null)
        {
            _room = room;
            _entityFields = entityFields;
        }

        public static void Detach()
        {
            _room = null;
            _entityFields = null;
        }

        /// Build rows from the core's decoded state. `delta` is applied to the
        /// attached room first (a schema delta decodes there); the returned
        /// rows mirror the core state for the primary collection, then the
        /// root scalars (tag 0x10). Without an attached room (no descriptor
        /// seen yet) the returned list is empty — decode never throws.
        public static List<CygRow> Decode(byte[] delta, IReadOnlyDictionary<byte, string[]> tagFields)
        {
            var rows = new List<CygRow>();
            var room = _room;
            if (room == null) return rows;

            if (delta != null && delta.Length > 0 && !Cyg.CygRoom.IsSchemaDescriptor(delta))
                room.ApplyPayload(delta);

            string[] names = null;
            tagFields?.TryGetValue(0x01, out names);

            uint count = room.EntityCount();
            for (uint i = 0; i < count; i++)
            {
                uint id = room.EntityIdAt(i);
                var fields = new List<CygField>();
                int schemaFields = room.SchemaFieldCount(0);
                for (int f = 0; f < schemaFields; f++)
                {
                    var field = new CygField();
                    field.Name = (_entityFields != null && f < _entityFields.Length)
                        ? _entityFields[f]
                        : room.SchemaFieldName(0, f);
                    switch (room.SchemaFieldKind(0, f))
                    {
                        case 8: field.Bool = room.GetBool(id, f); break;        // bool
                        case 10: field.Str = room.GetString(id, f); break;      // string
                        case 9: field.Long = room.GetI64(id, f);                // i64
                            field.Number = field.Long; break;
                        default: field.Number = room.GetF64(id, f); break;      // numeric
                    }
                    fields.Add(field);
                }
                // Named fields from the caller's map still win when the schema
                // metadata is narrower than the legacy tag map.
                if (names != null)
                {
                    for (int f = 0; f < fields.Count && f < names.Length; f++)
                        fields[f] = new CygField { Name = names[f], Number = fields[f].Number, Long = fields[f].Long, Str = fields[f].Str, Bool = fields[f].Bool };
                }
                rows.Add(new CygRow { Tag = 0x01, IsRoot = false, EntityId = id, Fields = fields.ToArray() });
            }

            int rootCount = room.RootFieldCount();
            if (rootCount > 0)
            {
                var fields = new List<CygField>();
                string[] rootNames = null;
                tagFields?.TryGetValue(0x10, out rootNames);
                for (int f = 0; f < rootCount; f++)
                {
                    var field = new CygField();
                    field.Name = rootNames != null && f < rootNames.Length
                        ? rootNames[f]
                        : room.RootFieldName(f);
                    field.Number = room.GetRootF64(f);
                    fields.Add(field);
                }
                rows.Add(new CygRow { Tag = 0x10, IsRoot = true, EntityId = 0, Fields = fields.ToArray() });
            }
            return rows;
        }
    }
}
