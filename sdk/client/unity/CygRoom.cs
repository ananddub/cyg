// CygRoom — Unity C# client binding over the cyg-core-rs native lib.
//
// Drop this file into Assets/, and the built native library into
// Plugins/x86_64/libcyg_core_rs.so (Linux), Plugins/x86_64/cyg_core_rs.dll
// (Windows) or Plugins/cyg_core_rs.bundle (macOS) — build with
// `cargo build -p cyg-core-rs --release` (see sdk/README.md).
//
// Usage:
//   var room = new CygRoom(descriptorBytes);
//   byte[] frame = ...;                        // raw room-data payload
//   int events = room.ApplyPayload(frame);
//   float x = room.GetF64(entityId, room.FieldIndex("x"));
//   byte[] input = room.EncodeSchemaInput(seq, new long[]{ dirBits });

using System;
using System.Runtime.InteropServices;
// MonoPInvokeCallback (iOS IL2CPP) is only needed for Rust->C# callbacks;
// there are none in this ABI, so no AOT attribute is required here.

namespace Cyg
{
    public sealed class CygRoom : IDisposable
    {
        private uint _handle;

        public CygRoom(byte[] descriptor)
        {
            _handle = cyg_create(descriptor, (UIntPtr)descriptor.Length);
            if (_handle == 0)
                throw new ArgumentException("bad schema descriptor for cyg-core");
        }

        ~CygRoom() { Dispose(false); }

        public void Dispose() { Dispose(true); GC.SuppressFinalize(this); }
        private void Dispose(bool disposing)
        {
            if (_handle != 0)
            {
                cyg_free(_handle);
                _handle = 0;
            }
        }

        /// Apply one raw room-data payload; returns the number of state
        /// events produced, or -1 on a decode error (non-delta frames are
        /// skipped by the core and return 0).
        public int ApplyPayload(byte[] payload)
        {
            return cyg_apply_payload(_handle, payload, (UIntPtr)payload.Length);
        }

        /// f64 view of one entity field by schema-field index.
        public double GetF64(uint entityId, int fieldIndex)
            => cyg_get_f64_by_index(_handle, entityId, (UIntPtr)fieldIndex);

        /// String field; null when the field is not a string.
        public string GetString(uint entityId, int fieldIndex)
        {
            IntPtr n = cyg_get_string_len(_handle, entityId, (UIntPtr)fieldIndex);
            if (n.ToInt64() < 0) return null;
            byte[] buf = new byte[n.ToInt64()];
            IntPtr written = cyg_get_string_by_index(_handle, entityId, (UIntPtr)fieldIndex, buf, (UIntPtr)buf.Length);
            if (written.ToInt64() < 0) return null;
            return System.Text.Encoding.UTF8.GetString(buf, 0, (int)written.ToInt64());
        }

        /// Bool field; null when absent/not a bool.
        public bool? GetBool(uint entityId, int fieldIndex)
        {
            int v = cyg_get_bool_by_index(_handle, entityId, (UIntPtr)fieldIndex);
            return v < 0 ? (bool?)null : v == 1;
        }

        public uint EntityCount() => cyg_entity_count(_handle);
        public uint EntityIdAt(uint pos) => cyg_entity_id_at(_handle, pos);
        public int FieldIndex(string name) => cyg_field_index(_handle, name, (UIntPtr)name.Length);
        public bool StateReady() => cyg_state_ready(_handle) == 1;

        /// i64 field (0 when absent/not i64) — no f64 precision loss.
        public long GetI64(uint entityId, int fieldIndex)
            => cyg_get_i64_by_index(_handle, entityId, (UIntPtr)fieldIndex);

        // --- schema metadata (build a typed mirror without a second parser) --

        public int SchemaCount() => (int)cyg_schema_count(_handle);
        public string SchemaName(int index) => ReadStr((i, buf, cap) => cyg_schema_name(_handle, (uint)i, buf, (UIntPtr)cap), index);
        public uint SchemaHash(int index) => cyg_schema_hash(_handle, (uint)index);
        public uint SchemaVersion(int index) => cyg_schema_version(_handle, (uint)index);
        public int SchemaFieldCount(int index) => (int)cyg_schema_field_count(_handle, (uint)index);
        public string SchemaFieldName(int index, int field) =>
            ReadStr((i, buf, cap) => cyg_schema_field_name(_handle, (uint)index, (uint)field, buf, (UIntPtr)cap), index);
        /// Field kind discriminant: 0=u8 1=i8 2=u16 3=i16 4=u32 5=i32 6=f32
        /// 7=f64 8=bool 9=i64 10=string 11=array.
        public int SchemaFieldKind(int index, int field) => cyg_schema_field_kind(_handle, (uint)index, (uint)field);
        public int SchemaFieldFlags(int index, int field) => cyg_schema_field_flags(_handle, (uint)index, (uint)field);
        public int SchemaFieldElementKind(int index, int field) => cyg_schema_field_element_kind(_handle, (uint)index, (uint)field);

        // --- root scalars (kind-3/4 world frames) -----------------------------

        public int RootFieldCount() => (int)cyg_root_field_count(_handle);
        public string RootFieldName(int index) => ReadStr((i, buf, cap) => cyg_root_field_name(_handle, (uint)i, buf, (UIntPtr)cap), index);
        public double GetRootF64(int index) => cyg_get_root_f64(_handle, (UIntPtr)index);
        public string GetRootString(int index)
        {
            byte[] buf = new byte[256];
            int n = cyg_get_root_string(_handle, (UIntPtr)index, buf, (UIntPtr)buf.Length);
            return n < 0 ? null : System.Text.Encoding.UTF8.GetString(buf, 0, n);
        }
        public bool? GetRootBool(int index)
        {
            int v = cyg_get_root_bool(_handle, (UIntPtr)index);
            return v < 0 ? (bool?)null : v == 1;
        }

        /// "OXSD" magic check — the descriptor frame the host pushes after JOIN.
        public static bool IsSchemaDescriptor(byte[] bytes)
            => bytes != null && bytes.Length >= 4
                && bytes[0] == 0x4F && bytes[1] == 0x58 && bytes[2] == 0x53 && bytes[3] == 0x44;

        private delegate int NameFn(int index, byte[] buf, int cap);
        private static string ReadStr(NameFn fn, int index)
        {
            byte[] buf = new byte[128];
            int n = fn(index, buf, buf.Length);
            return n < 0 ? null : System.Text.Encoding.UTF8.GetString(buf, 0, Math.Min(n, buf.Length));
        }

        /// Encode a schema-lane input frame. Values are raw i64s; for f32/f64
        /// fields pass the IEEE bit pattern (e.g. BitConverter.DoubleToInt64Bits).
        public byte[] EncodeSchemaInput(ushort seq, long[] values)
        {
            IntPtr len;
            int rc;
            // two-pass: size probe then fill (outLen == -2 means cap too small)
            byte[] buf = new byte[64];
            rc = cyg_encode_schema_input(_handle, seq, values, (UIntPtr)values.Length, buf, (UIntPtr)buf.Length, out len);
            if (rc == -2)
            {
                buf = new byte[len.ToInt64()];
                rc = cyg_encode_schema_input(_handle, seq, values, (UIntPtr)values.Length, buf, (UIntPtr)buf.Length, out len);
            }
            if (rc != 0) return null;
            Array.Resize(ref buf, (int)len.ToInt64());
            return buf;
        }

        // ------------------------------------------------------------------
        // Native imports (libcyg_core_rs).
        // ------------------------------------------------------------------
        const string Lib = "cyg_core_rs";

        [DllImport(Lib)] static extern uint cyg_abi_version();
        [DllImport(Lib)] static extern uint cyg_create(byte[] descriptor, UIntPtr len);
        [DllImport(Lib)] static extern void cyg_free(uint handle);
        [DllImport(Lib)] static extern int cyg_apply_payload(uint handle, byte[] payload, UIntPtr len);
        [DllImport(Lib)] static extern double cyg_get_f64_by_index(uint handle, uint entity_id, UIntPtr index);
        [DllImport(Lib)] static extern IntPtr cyg_get_string_len(uint handle, uint entity_id, UIntPtr index);
        [DllImport(Lib)] static extern IntPtr cyg_get_string_by_index(uint handle, uint entity_id, UIntPtr index, byte[] out_buf, UIntPtr cap);
        [DllImport(Lib)] static extern int cyg_get_bool_by_index(uint handle, uint entity_id, UIntPtr index);
        [DllImport(Lib)] static extern uint cyg_entity_count(uint handle);
        [DllImport(Lib)] static extern uint cyg_entity_id_at(uint handle, uint pos);
        [DllImport(Lib)] static extern int cyg_field_index(uint handle, string name, UIntPtr len);
        [DllImport(Lib)] static extern int cyg_state_ready(uint handle);
        [DllImport(Lib)] static extern int cyg_encode_schema_input(uint handle, ushort seq, long[] values, UIntPtr len, byte[] out_buf, UIntPtr cap, out IntPtr out_len);
        [DllImport(Lib)] static extern long cyg_get_i64_by_index(uint handle, uint entity_id, UIntPtr index);
        [DllImport(Lib)] static extern uint cyg_schema_count(uint handle);
        [DllImport(Lib)] static extern int cyg_schema_name(uint handle, uint index, byte[] out_buf, UIntPtr cap);
        [DllImport(Lib)] static extern uint cyg_schema_hash(uint handle, uint index);
        [DllImport(Lib)] static extern uint cyg_schema_version(uint handle, uint index);
        [DllImport(Lib)] static extern uint cyg_schema_field_count(uint handle, uint index);
        [DllImport(Lib)] static extern int cyg_schema_field_name(uint handle, uint index, uint field, byte[] out_buf, UIntPtr cap);
        [DllImport(Lib)] static extern int cyg_schema_field_kind(uint handle, uint index, uint field);
        [DllImport(Lib)] static extern int cyg_schema_field_flags(uint handle, uint index, uint field);
        [DllImport(Lib)] static extern int cyg_schema_field_element_kind(uint handle, uint index, uint field);
        [DllImport(Lib)] static extern uint cyg_root_field_count(uint handle);
        [DllImport(Lib)] static extern int cyg_root_field_name(uint handle, uint index, byte[] out_buf, UIntPtr cap);
        [DllImport(Lib)] static extern double cyg_get_root_f64(uint handle, UIntPtr index);
        [DllImport(Lib)] static extern int cyg_get_root_string(uint handle, UIntPtr index, byte[] out_buf, UIntPtr cap);
        [DllImport(Lib)] static extern int cyg_get_root_bool(uint handle, UIntPtr index);
    }
}
