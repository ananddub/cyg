mod descriptor;
mod emit;
mod model;
mod options;
mod validate;
mod wire;

pub mod codegen;
pub mod ts;

#[cfg(test)]
mod tests;

pub use descriptor::{
    DESCRIPTOR_MAGIC, DESCRIPTOR_VERSION, Descriptor, emit_descriptor, parse_descriptor,
};
pub use model::{
    GeneratedSchema, MAX_COLLECTIONS, RootCollection, RootState, SchemaDef, SchemaDiagnostic,
    SchemaField, generate_with_root, wire_hash,
};
pub use options::encode_join_options;
pub use wire::{
    FIELD_OPTIONAL, FIELD_STATIC, FIELD_TRANSIENT, WireKind, array_assemblyscript_type,
    array_typescript_type,
};

/// A schema def with this name is the per-tick input frame: codegen emits a
/// decoder for it, the client packs `room.sendInput({...})` from it, and the
/// name may not double as a state entity.
pub const INPUT_SCHEMA_NAME: &str = "Input";

/// A schema def with this name types the join options: the host re-encodes
/// the client's JSON into this field order, codegen emits
/// `decodeJoinOptions(r)`, and the room's `onJoin(id, options)` is typed.
pub const JOIN_OPTIONS_SCHEMA_NAME: &str = "JoinOptions";

pub(crate) use emit::{emit_client_dts, emit_server};
pub(crate) use validate::{
    MAX_FIELDS, check_schema_name, check_unique_names, is_reserved_field_name,
};
