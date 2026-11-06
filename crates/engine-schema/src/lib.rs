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

pub use options::encode_join_options;
pub use descriptor::{
    emit_descriptor, parse_descriptor, Descriptor, DESCRIPTOR_MAGIC, DESCRIPTOR_VERSION,
};
pub use model::{
    generate_with_root, wire_hash, GeneratedSchema, RootCollection, RootState, SchemaDef,
    SchemaDiagnostic, SchemaField, MAX_COLLECTIONS,
};
pub use wire::{
    array_assemblyscript_type, array_typescript_type, WireKind, FIELD_OPTIONAL, FIELD_STATIC,
    FIELD_TRANSIENT,
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
    check_schema_name, check_unique_names, is_reserved_field_name, MAX_FIELDS,
};
