//! Host-side transcoder: the guest's fixed-width OXS2 sections → the packed
//! wire the clients receive.
//!
//! State is produced inside the guest wasm, so the host cannot encode from
//! values it never had. What it *can* do is read the bytes the guest already
//! produced and re-encode them. The guest ABI stays fixed-width on purpose:
//! every server SDK (AssemblyScript, TypeScript, Rust, C, C++, C#, Lua) keeps
//! emitting the same simple section, and the host upgrades it once.
//!
//! This is the *only* state wire format now. There is no fixed-width fallback:
//! a descriptor compiles into one [`PackedSet`] at boot, and every peer, lane,
//! AOI view and spectator frame goes through it.
//!
//! Cost model: one decode plus one encode per room tick for the shared body,
//! and one extra transcode for a per-session AOI view. Nothing here runs per
//! peer when the view is shared.

use engine_schema::{Descriptor, SchemaDef};
use engine_webrtc_protocol::packed::{self, PackedSchema};

use super::legacy_section;

/// Every schema in one descriptor, compiled once. The root is a synthetic
/// [`SchemaDef`] over `descriptor.root.scalars` — the same definition the guest
/// and the client use for the world root hash.
pub struct PackedSet {
    root_schema: SchemaDef,
    root: PackedSchema,
    schemas: Vec<(u32, SchemaDef, PackedSchema)>,
    pub clamped: u32,
    pub failures: u64,
}

impl PackedSet {
    /// Builds the transcoder for a descriptor. Returns `None` when the
    /// descriptor has no schemas at all (a relay room with no state wire).
    pub fn from_descriptor(bytes: &[u8]) -> Option<Self> {
        let descriptor: Descriptor = engine_schema::parse_descriptor(bytes)?;
        if descriptor.schemas.is_empty() {
            return None;
        }
        let root_schema = SchemaDef {
            name: descriptor.root.name.clone(),
            version: descriptor.root.version,
            fields: descriptor.root.scalars.clone(),
            line: 0,
            column: 0,
        };
        let root = PackedSchema::from_def(&root_schema);
        let schemas = descriptor
            .schemas
            .into_iter()
            .map(|schema| {
                let hash = engine_schema::wire_hash(&schema);
                (hash, schema.clone(), PackedSchema::from_def(&schema))
            })
            .collect();
        Some(Self {
            root_schema,
            root,
            schemas,
            clamped: 0,
            failures: 0,
        })
    }

    fn schema_by_hash(&self, hash: u32) -> Option<&SchemaDef> {
        self.schemas
            .iter()
            .find(|(candidate, _, _)| *candidate == hash)
            .map(|(_, schema, _)| schema)
    }

    fn packed_by_hash(&self, hash: u32) -> Option<&PackedSchema> {
        self.schemas
            .iter()
            .find(|(candidate, _, _)| *candidate == hash)
            .map(|(_, _, packed)| packed)
    }

    fn first_packed(&self) -> Option<&PackedSchema> {
        self.schemas.first().map(|(_, _, packed)| packed)
    }

    /// Re-encodes a guest full section (collection kind 1, or world kind 3).
    pub fn full(&mut self, section: &[u8]) -> Result<Vec<u8>, String> {
        let outcome = self.transcode(section, true);
        self.counted(outcome)
    }

    /// Re-encodes a guest patch section (collection kind 2, or world kind 4).
    pub fn patch(&mut self, section: &[u8]) -> Result<Vec<u8>, String> {
        let outcome = self.transcode(section, false);
        self.counted(outcome)
    }

    fn transcode(&mut self, section: &[u8], full: bool) -> Result<Vec<u8>, String> {
        let kind = *section.get(4).ok_or("truncated schema section")?;
        match kind {
            legacy_section::KIND_FULL | legacy_section::KIND_PATCH => {
                self.transcode_collection(section, full)
            }
            legacy_section::KIND_WORLD_FULL | legacy_section::KIND_WORLD_PATCH => {
                self.transcode_world(section, full)
            }
            other => Err(format!("unknown guest schema kind {other}")),
        }
    }

    fn transcode_collection(&mut self, section: &[u8], full: bool) -> Result<Vec<u8>, String> {
        // The section carries its schema hash at bytes 5..9; pick that schema
        // instead of assuming the first collection (a descriptor can hold many).
        let hash = section
            .get(5..9)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or("truncated schema header")?;
        let schema = self
            .schema_by_hash(hash)
            .cloned()
            .or_else(|| self.schemas.first().map(|(_, schema, _)| schema.clone()))
            .ok_or("descriptor has no schema")?;
        let packed = PackedSchema::from_def(&schema);
        if full {
            let rows = legacy_section::decode_full(section, &schema)?;
            packed::encode_full(&packed, &rows, &mut self.clamped)
                .map_err(|error| error.to_string())
        } else {
            let ops = legacy_section::decode_patch(section, &schema)?;
            packed::encode_patch(&packed, &ops, &mut self.clamped)
                .map_err(|error| error.to_string())
        }
    }

    fn transcode_world(&mut self, section: &[u8], full: bool) -> Result<Vec<u8>, String> {
        if full {
            let world = legacy_section::decode_world_full(section, &self.root_schema)?;
            let mut collections = Vec::with_capacity(world.collections.len());
            for collection in world.collections {
                let schema = self
                    .schema_by_hash(collection.hash)
                    .cloned()
                    .ok_or_else(|| format!("unknown collection schema {:08x}", collection.hash))?;
                let packed = self
                    .packed_by_hash(collection.hash)
                    .cloned()
                    .ok_or_else(|| format!("unknown collection schema {:08x}", collection.hash))?;
                let rows = legacy_section::decode_full(collection.section, &schema)?;
                let bytes = packed::encode_full(&packed, &rows, &mut self.clamped)
                    .map_err(|error| error.to_string())?;
                collections.push((collection.id, collection.hash, bytes));
            }
            packed::encode_world_full(&self.root, &world.root, &collections, &mut self.clamped)
                .map_err(|error| error.to_string())
        } else {
            let world = legacy_section::decode_world_patch(section, &self.root_schema)?;
            let mut collections = Vec::with_capacity(world.collections.len());
            for collection in world.collections {
                let schema = self
                    .schema_by_hash(collection.hash)
                    .cloned()
                    .ok_or_else(|| format!("unknown collection schema {:08x}", collection.hash))?;
                let packed = self
                    .packed_by_hash(collection.hash)
                    .cloned()
                    .ok_or_else(|| format!("unknown collection schema {:08x}", collection.hash))?;
                let ops = legacy_section::decode_patch(collection.section, &schema)?;
                let bytes = packed::encode_patch(&packed, &ops, &mut self.clamped)
                    .map_err(|error| error.to_string())?;
                collections.push((collection.id, collection.hash, bytes));
            }
            packed::encode_world_patch(
                &self.root,
                world.root_mask,
                &world.root,
                &collections,
                &mut self.clamped,
            )
            .map_err(|error| error.to_string())
        }
    }

    /// The schema hash of the primary collection, used by the old call sites
    /// and by the boot log. `None` only when the descriptor had no schemas
    /// (which `from_descriptor` already rejects).
    pub fn hash(&self) -> Option<u32> {
        self.first_packed().map(PackedSchema::hash)
    }

    /// Every failure path bumps the counter — a silent transcode failure would
    /// look like "the room sends no state", the hardest bug to trace back here.
    fn counted(&mut self, outcome: Result<Vec<u8>, String>) -> Result<Vec<u8>, String> {
        if outcome.is_err() {
            self.failures += 1;
        }
        outcome
    }
}

#[cfg(test)]
mod tests;
