mod descriptor;
mod emit;
mod model;
mod validate;
mod wire;

pub mod codegen;
pub mod ts;

#[cfg(test)]
mod tests;

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

pub(crate) use emit::{emit_client_dts, emit_server};
pub(crate) use validate::{
    check_schema_name, check_unique_names, is_reserved_field_name, MAX_FIELDS,
};
