//! Host-side packed-section validator and per-view masker.
//!
//! The guest emits canonical packed OXS2 sections. Shared state passes through
//! byte-for-byte; viewer-specific private/AOI masking decodes and re-encodes.
//!
//! This is the *only* state wire format now. There is no fixed-width fallback:
//! a descriptor compiles into one [`PackedSet`] at boot, and every peer, lane,
//! AOI view and spectator frame goes through it.
//!
//! Cost model: shared frames are decoded for validation and then passed through;
//! per-session views decode, mask, and re-encode only when required.

use std::collections::{HashMap, HashSet};

use engine_schema::{Descriptor, SchemaDef};
use engine_webrtc_protocol::packed::{self, PackedSchema};

use super::view::{VIEW_ROOT_COLLECTION, ViewIntent, ViewMask};

/// Default per-view patch byte budget. A patch over this keeps its low-priority
/// updates in a per-session deferred queue instead of losing them.
pub const DEFAULT_PATCH_BUDGET: usize = 1200;
/// Bounds for the bandwidth-derived per-session budget.
pub const MIN_PATCH_BUDGET: usize = 256;
pub const MAX_PATCH_BUDGET: usize = 8192;

/// Turn the reliable lane's pacing estimate into a per-tick patch budget.
/// `pacing_bps` is bytes/second. A missing/zero estimate keeps the old fixed
/// default, so the first packets of a session behave exactly as before.
pub fn adaptive_patch_budget(pacing_bps: f64, tick: std::time::Duration) -> usize {
    if !pacing_bps.is_finite() || pacing_bps <= 0.0 {
        return DEFAULT_PATCH_BUDGET;
    }
    // A snapshot is roughly one eighth of a tick's paced budget; the snapshot
    // itself is only part of what the lane carries.
    let bytes = pacing_bps * tick.as_secs_f64() / 8.0;
    (bytes.round() as usize).clamp(MIN_PATCH_BUDGET, MAX_PATCH_BUDGET)
}

/// Every schema in one descriptor, compiled once. The root is a synthetic
/// [`SchemaDef`] over `descriptor.root.scalars` — the same definition the guest
/// and the client use for the world root hash.
pub struct PackedSet {
    root_schema: SchemaDef,
    root: PackedSchema,
    schemas: Vec<(u32, SchemaDef, PackedSchema)>,
    /// Collection ids that are nested child collections; their row id packs
    /// `(owner << 16) | slot`, so `.private()` ownership checks the high bits.
    nested_ids: HashSet<u16>,
    /// Name -> root collection id, and root collection id -> schema name.
    collection_ids: HashMap<String, u16>,
    collection_schemas: Vec<String>,
    /// Schema hash -> the first root collection using it. A standalone
    /// collection section carries only its schema hash; this lets a view hide
    /// the named collection's fields there too.
    hash_collection: HashMap<u32, u16>,
    /// Schema name -> field name -> wire field index.
    field_indices: HashMap<String, HashMap<String, u32>>,
    root_field_indices: HashMap<String, u32>,
    /// Field bits marked `.unreliable()` per schema hash, and for the root.
    unreliable_by_hash: HashMap<u32, u32>,
    root_unreliable: u32,
    /// Latest values of volatile fields, kept host-side so the unreliable
    /// stream can be rebuilt without asking the guest again.
    volatile_root: Vec<packed::WireValue>,
    volatile_collections: HashMap<(u16, u32), HashMap<u32, Vec<packed::WireValue>>>,
    /// Whether the guest emits the single-collection or world form. Set by the
    /// first section, so the volatile stream mirrors the same shape.
    world_mode: bool,
    /// Updates that did not fit a per-view budget, keyed by room + viewer +
    /// collection. They are re-merged on the next patch; a newer op for the
    /// same id wins.
    deferred: HashMap<(String, u32, u16), Vec<packed::PackedOp>>,
    pub clamped: u32,
    pub failures: u64,
}

impl PackedSet {
    /// Builds packed schemas for a descriptor. Returns `None` when the
    /// descriptor has no schemas at all (a relay room with no state wire).
    pub fn from_descriptor(bytes: &[u8]) -> Option<Self> {
        let descriptor: Descriptor = engine_schema::parse_descriptor(bytes)?;
        if descriptor.schemas.is_empty() {
            return None;
        }
        let root_schema = SchemaDef {
            nested_arrays: Vec::new(),
            name: descriptor.root.name.clone(),
            version: descriptor.root.version,
            fields: descriptor.root.scalars.clone(),
            line: 0,
            column: 0,
        };
        let root = PackedSchema::from_def(&root_schema);
        let nested_ids: HashSet<u16> = descriptor
            .root
            .nested
            .iter()
            .filter_map(|nested| {
                descriptor
                    .root
                    .collections
                    .iter()
                    .position(|collection| collection.name == nested.collection)
                    .map(|index| index as u16)
            })
            .collect();
        let collection_ids: HashMap<String, u16> = descriptor
            .root
            .collections
            .iter()
            .enumerate()
            .map(|(index, collection)| (collection.name.clone(), index as u16))
            .collect();
        let collection_schemas = descriptor
            .root
            .collections
            .iter()
            .map(|collection| collection.schema.clone())
            .collect();
        let field_indices: HashMap<String, HashMap<String, u32>> = descriptor
            .schemas
            .iter()
            .map(|schema| {
                let fields = schema
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(index, field)| (field.name.clone(), index as u32))
                    .collect();
                (schema.name.clone(), fields)
            })
            .collect();
        let root_field_indices = descriptor
            .root
            .scalars
            .iter()
            .enumerate()
            .map(|(index, field)| (field.name.clone(), index as u32))
            .collect();
        let mut hash_collection = HashMap::new();
        for (index, collection) in descriptor.root.collections.iter().enumerate() {
            if let Some(schema) = descriptor
                .schemas
                .iter()
                .find(|schema| schema.name == collection.schema)
            {
                hash_collection
                    .entry(engine_schema::wire_hash(schema))
                    .or_insert(index as u16);
            }
        }
        let mut unreliable_by_hash = HashMap::new();
        for schema in &descriptor.schemas {
            let bits = unreliable_bits(schema);
            if bits != 0 {
                unreliable_by_hash.insert(engine_schema::wire_hash(schema), bits);
            }
        }
        let root_unreliable = unreliable_bits(&root_schema);
        let root_field_count = root_schema.fields.len();
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
            nested_ids,
            collection_ids,
            collection_schemas,
            hash_collection,
            field_indices,
            root_field_indices,
            unreliable_by_hash,
            root_unreliable,
            volatile_root: vec![packed::WireValue::Null; root_field_count],
            volatile_collections: HashMap::new(),
            world_mode: false,
            deferred: HashMap::new(),
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

    /// True when this schema has an owner-only field. That makes the encoded
    /// body viewer-dependent, so the shared per-tick frame cannot be reused.
    pub fn has_private_fields(&self) -> bool {
        self.root_schema
            .fields
            .iter()
            .any(engine_schema::SchemaField::private)
            || self.schemas.iter().any(|(_, schema, _)| {
                schema
                    .fields
                    .iter()
                    .any(engine_schema::SchemaField::private)
            })
    }

    /// True when the descriptor marks any field `.unreliable()`.
    pub fn has_unreliable_fields(&self) -> bool {
        self.root_unreliable != 0 || !self.unreliable_by_hash.is_empty()
    }

    fn unreliable_bits_for(&self, hash: u32) -> u32 {
        self.unreliable_by_hash.get(&hash).copied().unwrap_or(0)
    }

    fn collection_key(&self, hash: u32, fallback: u16) -> (u16, u32) {
        (self.hash_collection.get(&hash).copied().unwrap_or(fallback), hash)
    }

    fn cache_collection(
        &mut self,
        collection: u16,
        hash: u32,
        decoded: &packed::Decoded,
        full: bool,
    ) {
        let bits = self.unreliable_bits_for(hash);
        if bits == 0 {
            return;
        }
        let key = self.collection_key(hash, collection);
        if full {
            let mut rows = HashMap::new();
            for op in &decoded.ops {
                if let packed::PackedOp::Add { id, values } = op {
                    rows.insert(*id, values.clone());
                }
            }
            self.volatile_collections.insert(key, rows);
            return;
        }
        let row_len = self.schema_by_hash(hash).map_or(0, |s| s.fields.len());
        let rows = self.volatile_collections.entry(key).or_default();
        for op in &decoded.ops {
            match op {
                packed::PackedOp::Add { id, values } => {
                    rows.insert(*id, values.clone());
                }
                packed::PackedOp::Remove { id } => {
                    rows.remove(id);
                }
                packed::PackedOp::Update { id, values, .. } => {
                    let row = rows
                        .entry(*id)
                        .or_insert_with(|| vec![packed::WireValue::Null; row_len]);
                    for (index, value) in values {
                        if *index < row.len() {
                            row[*index] = value.clone();
                        }
                    }
                }
            }
        }
    }

    fn cache_world(&mut self, decoded: &packed::DecodedWorld, full: bool) {
        if self.root_unreliable != 0 {
            if full {
                self.volatile_root = vec![packed::WireValue::Null; self.root_schema.fields.len()];
                for (index, value) in &decoded.root {
                    if *index < self.volatile_root.len() {
                        self.volatile_root[*index] = value.clone();
                    }
                }
            } else {
                for (index, value) in &decoded.root {
                    if *index < self.volatile_root.len() {
                        self.volatile_root[*index] = value.clone();
                    }
                }
            }
        }
        for (id, hash, bytes) in &decoded.collections {
            let bits = self.unreliable_bits_for(*hash);
            if bits == 0 {
                continue;
            }
            let Some(packed_schema) = self.packed_by_hash(*hash) else {
                continue;
            };
            let Ok(inner) = packed::decode(packed_schema, bytes) else {
                continue;
            };
            self.cache_collection(*id, *hash, &inner, full);
        }
    }

    /// Builds the shared volatile-only patch from the host cache. Marked fields
    /// are sent on the unreliable stream every tick; must-deliver fields never
    /// appear here. Only safe when no field is owner-only and no view is active.
    pub fn volatile_patch(&mut self) -> Option<Vec<u8>> {
        self.volatile_patch_view_impl(None, None)
    }

    /// Builds a volatile-only patch already masked for one viewer. Used when
    /// `.private()` or a view can hide a volatile field; an unmasked patch
    /// would leak it to every peer.
    pub fn volatile_patch_view(
        &mut self,
        viewer: u32,
        view: Option<&ViewMask>,
    ) -> Option<Vec<u8>> {
        self.volatile_patch_view_impl(Some(viewer), view)
    }

    fn volatile_patch_view_impl(
        &mut self,
        viewer: Option<u32>,
        view: Option<&ViewMask>,
    ) -> Option<Vec<u8>> {
        if !self.has_unreliable_fields() {
            return None;
        }
        if self.world_mode {
            let mut collections = Vec::new();
            let mut entries: Vec<(u16, u32, HashMap<u32, Vec<packed::WireValue>>)> =
                self.volatile_collections
                    .iter()
                    .map(|((id, hash), rows)| (*id, *hash, rows.clone()))
                    .collect();
            entries.sort_by_key(|(id, hash, _)| (*id, *hash));
            for (id, hash, rows) in entries {
                let bits = self.unreliable_bits_for(hash);
                if bits == 0 || rows.is_empty() {
                    continue;
                }
                let Some(packed_schema) = self.packed_by_hash(hash) else {
                    continue;
                };
                let Some(schema) = self.schema_by_hash(hash).cloned() else {
                    continue;
                };
                let mut ops = Vec::with_capacity(rows.len());
                for (row_id, values) in rows {
                    let entries: Vec<(usize, packed::WireValue)> = values
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| bits & (1u32 << index) != 0)
                        .map(|(index, value)| (index, value.clone()))
                        .collect();
                    if !entries.is_empty() {
                        ops.push(packed::PackedOp::Update {
                            id: row_id,
                            mask: bits,
                            values: entries,
                        });
                    }
                }
                if ops.is_empty() {
                    continue;
                }
                if let Some(viewer) = viewer {
                    let shift = self.nested_ids.contains(&id).then_some(16);
                    let hidden = view.map(|view| view.hidden_for(Some(id))).unwrap_or(0);
                    mask_ops(&schema, &mut ops, viewer, shift, view, hidden);
                }
                if ops.is_empty() {
                    continue;
                }
                let mut clamped = 0;
                if let Ok(bytes) = packed::encode_patch(packed_schema, &ops, &mut clamped) {
                    self.clamped += clamped;
                    collections.push((id, hash, bytes));
                }
            }
            let mut root_mask = self.root_unreliable;
            let mut root_entries: Vec<(usize, packed::WireValue)> = self
                .volatile_root
                .iter()
                .enumerate()
                .filter(|(index, _)| root_mask & (1u32 << index) != 0)
                .map(|(index, value)| (index, value.clone()))
                .collect();
            if let Some(viewer) = viewer {
                let hidden = view.map(ViewMask::hidden_root).unwrap_or(0);
                mask_root_patch(
                    &self.root_schema,
                    &mut root_entries,
                    &mut root_mask,
                    viewer,
                    hidden,
                );
            }
            if root_mask == 0 && collections.is_empty() {
                return None;
            }
            let mut clamped = 0;
            let out = packed::encode_world_patch(
                &self.root,
                root_mask,
                &root_entries,
                &collections,
                &mut clamped,
            )
            .ok();
            self.clamped += clamped;
            return out;
        }
        // Single-collection form: the first schema that declares volatile
        // fields, with every cached row carrying those fields.
        let schemas: Vec<(u32, u16)> = self
            .unreliable_by_hash
            .keys()
            .filter_map(|hash| {
                self.hash_collection
                    .get(hash)
                    .or(Some(&0))
                    .map(|id| (*hash, *id))
            })
            .collect();
        let (hash, collection) = schemas.into_iter().next()?;
        let bits = self.unreliable_bits_for(hash);
        let packed_schema = self.packed_by_hash(hash)?;
        let schema = self.schema_by_hash(hash)?.clone();
        let rows = self.volatile_collections.get(&self.collection_key(hash, collection))?;
        if rows.is_empty() {
            return None;
        }
        let mut ops = Vec::with_capacity(rows.len());
        for (id, values) in rows {
            let entries: Vec<(usize, packed::WireValue)> = values
                .iter()
                .enumerate()
                .filter(|(index, _)| bits & (1u32 << index) != 0)
                .map(|(index, value)| (index, value.clone()))
                .collect();
            if !entries.is_empty() {
                ops.push(packed::PackedOp::Update {
                    id: *id,
                    mask: bits,
                    values: entries,
                });
            }
        }
        if ops.is_empty() {
            return None;
        }
        if let Some(viewer) = viewer {
            let shift = self.nested_ids.contains(&collection).then_some(16);
            let hidden = view.map(|view| view.hidden_for(Some(collection))).unwrap_or(0);
            mask_ops(&schema, &mut ops, viewer, shift, view, hidden);
        }
        if ops.is_empty() {
            return None;
        }
        let mut clamped = 0;
        let out = packed::encode_patch(packed_schema, &ops, &mut clamped).ok();
        self.clamped += clamped;
        out
    }

    /// Validates and passes through a guest packed full section (kind 5 or 7).
    pub fn full(&mut self, section: &[u8]) -> Result<Vec<u8>, String> {
        let outcome = self.process(section, true, None, None);
        self.counted(outcome)
    }

    /// Validates and passes through a guest packed patch section (kind 6 or 8).
    pub fn patch(&mut self, section: &[u8]) -> Result<Vec<u8>, String> {
        let outcome = self.process(section, false, None, None);
        self.counted(outcome)
    }

    /// Full section with `.private()` fields masked for `viewer_id`.
    pub fn full_for(&mut self, section: &[u8], viewer_id: u32) -> Result<Vec<u8>, String> {
        let outcome = self.process(section, true, Some(viewer_id), None);
        self.counted(outcome)
    }

    /// Patch section with `.private()` changes removed for `viewer_id`.
    pub fn patch_for(&mut self, section: &[u8], viewer_id: u32) -> Result<Vec<u8>, String> {
        let outcome = self.process(section, false, Some(viewer_id), None);
        self.counted(outcome)
    }

    /// Full section filtered by the viewer's session [`ViewMask`].
    pub fn full_for_view(
        &mut self,
        section: &[u8],
        viewer_id: u32,
        view: &ViewMask,
    ) -> Result<Vec<u8>, String> {
        let outcome = self.process(section, true, Some(viewer_id), Some(view));
        self.counted(outcome)
    }

    /// Patch section filtered by the viewer's session [`ViewMask`].
    pub fn patch_for_view(
        &mut self,
        section: &[u8],
        viewer_id: u32,
        view: &ViewMask,
    ) -> Result<Vec<u8>, String> {
        let outcome = self.process(section, false, Some(viewer_id), Some(view));
        self.counted(outcome)
    }

    /// Patch with a per-view byte budget. Only `UPDATE` ops are deferred; an
    /// add/remove is structural and always ships. The guest already emits
    /// updates in priority order, so the suffix is the lowest priority.
    pub fn patch_for_view_budget(
        &mut self,
        room: &str,
        section: &[u8],
        viewer_id: u32,
        view: &ViewMask,
        budget: usize,
    ) -> Result<Vec<u8>, String> {
        let outcome = self.process_budget(room, section, Some(viewer_id), Some(view), budget);
        self.counted(outcome)
    }

    /// Patch with a per-view budget but only `.private()` masking.
    pub fn patch_for_budget(
        &mut self,
        room: &str,
        section: &[u8],
        viewer_id: u32,
        budget: usize,
    ) -> Result<Vec<u8>, String> {
        let outcome = self.process_budget(room, section, Some(viewer_id), None, budget);
        self.counted(outcome)
    }

    /// Drop a viewer's deferred updates (called after a full snapshot, which
    /// already contains every row).
    pub fn clear_deferred(&mut self, room: &str, viewer_id: u32) {
        self.deferred
            .retain(|(r, v, _), _| !(r == room && *v == viewer_id));
    }

    /// Resolves one guest intent against the descriptor and mutates the view.
    /// Returns whether the effective mask changed; an unchanged intent must not
    /// force another full snapshot.
    pub fn apply_view_intent(
        &self,
        view: &mut ViewMask,
        intent: &ViewIntent,
    ) -> Result<bool, String> {
        match intent {
            ViewIntent::HideField {
                collection, field, ..
            } => {
                let (collection_id, index) = if collection.as_ref() == VIEW_ROOT_COLLECTION {
                    let Some(index) = self.root_field_indices.get(field.as_ref()) else {
                        return Err(format!("unknown root field `{field}`"));
                    };
                    (None, *index)
                } else {
                    let Some(id) = self.collection_ids.get(collection.as_ref()).copied() else {
                        return Err(format!("unknown collection `{collection}`"));
                    };
                    let schema = self
                        .collection_schemas
                        .get(id as usize)
                        .ok_or_else(|| format!("collection `{collection}` has no schema"))?;
                    let Some(index) = self
                        .field_indices
                        .get(schema)
                        .and_then(|fields| fields.get(field.as_ref()))
                    else {
                        return Err(format!(
                            "unknown field `{field}` on collection `{collection}`"
                        ));
                    };
                    (Some(id), *index)
                };
                Ok(view.hide(collection_id, index))
            }
            ViewIntent::OnlySelf { enabled, .. } => Ok(view.set_only_self(*enabled)),
            ViewIntent::Reset { .. } => Ok(view.clear()),
        }
    }

    fn process_budget(
        &mut self,
        room: &str,
        section: &[u8],
        viewer: Option<u32>,
        view: Option<&ViewMask>,
        budget: usize,
    ) -> Result<Vec<u8>, String> {
        let kind = *section.get(4).ok_or("truncated schema section")?;
        match kind {
            packed::KIND_PATCH => {
                self.process_collection_budget(room, section, viewer, view, budget)
            }
            packed::KIND_WORLD_PATCH => {
                self.process_world_budget(room, section, viewer, view, budget)
            }
            // A full snapshot is already the authoritative baseline; budgeting
            // it would make the client start from a partial world.
            _ => self.process(section, false, viewer, view),
        }
    }

    fn process_world_budget(
        &mut self,
        room: &str,
        section: &[u8],
        viewer: Option<u32>,
        view: Option<&ViewMask>,
        budget: usize,
    ) -> Result<Vec<u8>, String> {
        // Mask/view/unreliable first, then budget each collection patch. The
        // root scalars are tiny and always ship.
        let masked = self.process(section, false, viewer, view)?;
        if masked.get(4).copied() != Some(packed::KIND_WORLD_PATCH) {
            return Ok(masked);
        }
        let decoded = packed::decode_world(&self.root, &masked)
            .map_err(|error| format!("invalid masked world patch: {error}"))?;
        let mut remaining = budget.saturating_sub(packed::WORLD_HEADER_LEN + 4 + 2);
        let mut collections = Vec::with_capacity(decoded.collections.len());
        for (id, hash, bytes) in decoded.collections {
            let share = remaining;
            let out = self.process_collection_budget(room, &bytes, viewer, view, share)?;
            remaining = remaining.saturating_sub(out.len().saturating_add(10));
            collections.push((id, hash, out));
        }
        let mut clamped = 0;
        let out = packed::encode_world_patch(
            &self.root,
            decoded.root_mask,
            &decoded.root,
            &collections,
            &mut clamped,
        )
        .map_err(|error| error.to_string());
        self.clamped += clamped;
        out
    }

    fn process_collection_budget(
        &mut self,
        room: &str,
        section: &[u8],
        viewer: Option<u32>,
        view: Option<&ViewMask>,
        budget: usize,
    ) -> Result<Vec<u8>, String> {
        let kind = *section.get(4).ok_or("truncated schema section")?;
        if !matches!(kind, packed::KIND_FULL | packed::KIND_PATCH) {
            return self.process(section, false, viewer, view);
        }
        let hash = section
            .get(5..9)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or("truncated schema header")?;
        let schema = self
            .schema_by_hash(hash)
            .cloned()
            .ok_or_else(|| format!("unknown guest collection schema {hash:08x}"))?;
        let packed = PackedSchema::from_def(&schema);
        let collection = self.hash_collection.get(&hash).copied();
        let shift = collection.and_then(|id| self.nested_ids.contains(&id).then_some(16));
        let hidden = view.map(|view| view.hidden_for(collection)).unwrap_or(0);
        let decoded = packed::decode(&packed, section)
            .map_err(|error| format!("invalid packed guest collection section: {error}"))?;
        if decoded.kind != packed::KIND_PATCH {
            return self.process(section, false, viewer, view);
        }
        let unreliable = self.unreliable_bits_for(hash);
        if unreliable != 0 {
            self.cache_collection(collection.unwrap_or(0), hash, &decoded, false);
        }
        let mut ops = decoded.ops;
        if let Some(viewer) = viewer {
            mask_ops(&schema, &mut ops, viewer, shift, view, hidden);
        }
        if unreliable != 0 {
            strip_unreliable_ops(&mut ops, unreliable);
        }
        let key = (
            room.to_string(),
            viewer.unwrap_or(u32::MAX),
            collection.unwrap_or(0),
        );
        let deferred = self.deferred.remove(&key).unwrap_or_default();
        // A fresh op for an id supersedes an older deferred op for that id.
        let fresh_ids: HashSet<u32> = ops.iter().map(op_id).collect();
        let mut merged: Vec<packed::PackedOp> = ops;
        for op in deferred {
            if !fresh_ids.contains(&op_id(&op)) {
                merged.push(op);
            }
        }
        let (out, dropped) = encode_with_budget(&packed, merged, budget, &mut self.clamped)?;
        if !dropped.is_empty() {
            self.deferred.insert(key, dropped);
        }
        Ok(out)
    }

    fn process(
        &mut self,
        section: &[u8],
        full: bool,
        viewer: Option<u32>,
        view: Option<&ViewMask>,
    ) -> Result<Vec<u8>, String> {
        let kind = *section.get(4).ok_or("truncated schema section")?;
        if matches!(kind, packed::KIND_WORLD_FULL | packed::KIND_WORLD_PATCH) {
            self.world_mode = true;
        }
        match kind {
            packed::KIND_FULL | packed::KIND_PATCH => {
                self.process_collection(section, full, viewer, view)
            }
            packed::KIND_WORLD_FULL | packed::KIND_WORLD_PATCH => {
                self.process_world(section, full, viewer, view)
            }
            1..=4 => Err(format!(
                "fixed-width guest schema kind {kind} rejected; rebuild the room wasm for packed OXS2 sections"
            )),
            other => Err(format!("unknown guest schema kind {other}")),
        }
    }

    fn process_collection(
        &mut self,
        section: &[u8],
        full: bool,
        viewer: Option<u32>,
        view: Option<&ViewMask>,
    ) -> Result<Vec<u8>, String> {
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
            .ok_or_else(|| format!("unknown guest collection schema {hash:08x}"))?;
        let packed = PackedSchema::from_def(&schema);
        let collection = self.hash_collection.get(&hash).copied();
        let shift = collection.and_then(|id| self.nested_ids.contains(&id).then_some(16));
        let hidden = view.map(|view| view.hidden_for(collection)).unwrap_or(0);
        let decoded = packed::decode(&packed, section)
            .map_err(|error| format!("invalid packed guest collection section: {error}"))?;
        let expected = if full {
            packed::KIND_FULL
        } else {
            packed::KIND_PATCH
        };
        if decoded.kind != expected {
            return Err(format!(
                "expected packed section kind {expected}, got {}",
                decoded.kind
            ));
        }
        let unreliable = self.unreliable_bits_for(hash);
        if unreliable != 0 {
            self.cache_collection(collection.unwrap_or(0), hash, &decoded, full);
        }
        let Some(viewer) = viewer else {
            if unreliable == 0 || full {
                return Ok(section.to_vec());
            }
            let mut ops = decoded.ops;
            strip_unreliable_ops(&mut ops, unreliable);
            return packed::encode_patch(&packed, &ops, &mut self.clamped)
                .map_err(|error| error.to_string());
        };
        if full {
            let mut rows = Vec::with_capacity(decoded.ops.len());
            for op in decoded.ops {
                let packed::PackedOp::Add { id, values } = op else {
                    return Err("non-add operation in packed full section".into());
                };
                rows.push((id, values));
            }
            mask_rows(&schema, &mut rows, viewer, shift, view, hidden);
            packed::encode_full(&packed, &rows, &mut self.clamped)
                .map_err(|error| error.to_string())
        } else {
            let mut ops = decoded.ops;
            mask_ops(&schema, &mut ops, viewer, shift, view, hidden);
            if unreliable != 0 {
                strip_unreliable_ops(&mut ops, unreliable);
            }
            packed::encode_patch(&packed, &ops, &mut self.clamped)
                .map_err(|error| error.to_string())
        }
    }

    fn process_world(
        &mut self,
        section: &[u8],
        full: bool,
        viewer: Option<u32>,
        view: Option<&ViewMask>,
    ) -> Result<Vec<u8>, String> {
        let decoded = packed::decode_world(&self.root, section)
            .map_err(|error| format!("invalid packed guest world section: {error}"))?;
        let expected = if full {
            packed::KIND_WORLD_FULL
        } else {
            packed::KIND_WORLD_PATCH
        };
        if decoded.kind != expected {
            return Err(format!(
                "expected packed world kind {expected}, got {}",
                decoded.kind
            ));
        }
        self.cache_world(&decoded, full);
        let Some(viewer) = viewer else {
            if !full && (self.root_unreliable != 0 || !self.unreliable_by_hash.is_empty()) {
                return self.reliable_world_patch(&decoded);
            }
            for (id, hash, bytes) in &decoded.collections {
                let schema = self
                    .packed_by_hash(*hash)
                    .ok_or_else(|| format!("unknown collection schema {hash:08x}"))?;
                packed::decode(schema, bytes)
                    .map_err(|error| format!("invalid packed collection {id}: {error}"))?;
            }
            return Ok(section.to_vec());
        };
        let root_hidden = view.map(ViewMask::hidden_root).unwrap_or(0);
        if full {
            let mut root_values = vec![packed::WireValue::Null; self.root_schema.fields.len()];
            for (index, value) in decoded.root {
                root_values[index] = value;
            }
            mask_root(&self.root_schema, &mut root_values, viewer, root_hidden);
            let mut collections = Vec::with_capacity(decoded.collections.len());
            for (id, hash, bytes) in decoded.collections {
                let schema = self
                    .schema_by_hash(hash)
                    .cloned()
                    .ok_or_else(|| format!("unknown collection schema {hash:08x}"))?;
                let packed_schema = self
                    .packed_by_hash(hash)
                    .cloned()
                    .ok_or_else(|| format!("unknown collection schema {hash:08x}"))?;
                let decoded_collection = packed::decode(&packed_schema, &bytes)
                    .map_err(|error| format!("invalid packed collection {id}: {error}"))?;
                if decoded_collection.kind != packed::KIND_FULL {
                    return Err(format!("expected packed full collection {id}"));
                }
                let mut rows = Vec::with_capacity(decoded_collection.ops.len());
                for op in decoded_collection.ops {
                    let packed::PackedOp::Add { id, values } = op else {
                        return Err("non-add operation in packed world full".into());
                    };
                    rows.push((id, values));
                }
                let shift = self.nested_ids.contains(&id).then_some(16);
                let hidden = view.map(|view| view.hidden_for(Some(id))).unwrap_or(0);
                mask_rows(&schema, &mut rows, viewer, shift, view, hidden);
                let bytes = packed::encode_full(&packed_schema, &rows, &mut self.clamped)
                    .map_err(|error| error.to_string())?;
                collections.push((id, hash, bytes));
            }
            packed::encode_world_full(&self.root, &root_values, &collections, &mut self.clamped)
                .map_err(|error| error.to_string())
        } else {
            let mut root_mask = decoded.root_mask;
            let mut root_values = decoded.root;
            mask_root_patch(
                &self.root_schema,
                &mut root_values,
                &mut root_mask,
                viewer,
                root_hidden,
            );
            if self.root_unreliable != 0 {
                root_mask &= !self.root_unreliable;
                root_values.retain(|(index, _)| self.root_unreliable & (1u32 << index) == 0);
            }
            let mut collections = Vec::with_capacity(decoded.collections.len());
            for (id, hash, bytes) in decoded.collections {
                let schema = self
                    .schema_by_hash(hash)
                    .cloned()
                    .ok_or_else(|| format!("unknown collection schema {hash:08x}"))?;
                let packed_schema = self
                    .packed_by_hash(hash)
                    .cloned()
                    .ok_or_else(|| format!("unknown collection schema {hash:08x}"))?;
                let decoded_collection = packed::decode(&packed_schema, &bytes)
                    .map_err(|error| format!("invalid packed collection {id}: {error}"))?;
                if decoded_collection.kind != packed::KIND_PATCH {
                    return Err(format!("expected packed patch collection {id}"));
                }
                let mut ops = decoded_collection.ops;
                let shift = self.nested_ids.contains(&id).then_some(16);
                let hidden = view.map(|view| view.hidden_for(Some(id))).unwrap_or(0);
                mask_ops(&schema, &mut ops, viewer, shift, view, hidden);
                let bits = self.unreliable_bits_for(hash);
                if bits != 0 {
                    strip_unreliable_ops(&mut ops, bits);
                }
                let bytes = packed::encode_patch(&packed_schema, &ops, &mut self.clamped)
                    .map_err(|error| error.to_string())?;
                collections.push((id, hash, bytes));
            }
            packed::encode_world_patch(
                &self.root,
                root_mask,
                &root_values,
                &collections,
                &mut self.clamped,
            )
            .map_err(|error| error.to_string())
        }
    }

    fn reliable_world_patch(&mut self, decoded: &packed::DecodedWorld) -> Result<Vec<u8>, String> {
        let mut root_mask = decoded.root_mask;
        let mut root_values = decoded.root.clone();
        if self.root_unreliable != 0 {
            root_mask &= !self.root_unreliable;
            root_values.retain(|(index, _)| self.root_unreliable & (1u32 << index) == 0);
        }
        let mut collections = Vec::with_capacity(decoded.collections.len());
        for (id, hash, bytes) in &decoded.collections {
            let bits = self.unreliable_bits_for(*hash);
            let packed_schema = self
                .packed_by_hash(*hash)
                .ok_or_else(|| format!("unknown collection schema {hash:08x}"))?;
            let mut inner = packed::decode(packed_schema, bytes)
                .map_err(|error| format!("invalid packed collection {id}: {error}"))?;
            if bits != 0 {
                strip_unreliable_ops(&mut inner.ops, bits);
            }
            let mut clamped = 0;
            let bytes = packed::encode_patch(packed_schema, &inner.ops, &mut clamped)
                .map_err(|error| error.to_string())?;
            self.clamped += clamped;
            collections.push((*id, *hash, bytes));
        }
        let mut clamped = 0;
        let out = packed::encode_world_patch(
            &self.root,
            root_mask,
            &root_values,
            &collections,
            &mut clamped,
        )
        .map_err(|error| error.to_string());
        self.clamped += clamped;
        out
    }

    /// The schema hash of the primary collection, used by the old call sites
    /// and by the boot log. `None` only when the descriptor had no schemas
    /// (which `from_descriptor` already rejects).
    pub fn hash(&self) -> Option<u32> {
        self.first_packed().map(PackedSchema::hash)
    }

    /// Every failure path bumps the counter — a silent packed-section failure would
    /// look like "the room sends no state", the hardest bug to trace back here.
    fn counted(&mut self, outcome: Result<Vec<u8>, String>) -> Result<Vec<u8>, String> {
        if outcome.is_err() {
            self.failures += 1;
        }
        outcome
    }
}

fn op_id(op: &packed::PackedOp) -> u32 {
    match op {
        packed::PackedOp::Add { id, .. }
        | packed::PackedOp::Remove { id }
        | packed::PackedOp::Update { id, .. } => *id,
    }
}

/// Encode a patch, dropping the lowest-priority trailing `UPDATE` ops until
/// it fits `budget`. Structural adds/removes are never dropped. Returns the
/// patch and the deferred ops in their original order.
fn encode_with_budget(
    packed: &PackedSchema,
    mut ops: Vec<packed::PackedOp>,
    budget: usize,
    clamped: &mut u32,
) -> Result<(Vec<u8>, Vec<packed::PackedOp>), String> {
    let mut dropped: Vec<packed::PackedOp> = Vec::new();
    loop {
        let out = packed::encode_patch(packed, &ops, clamped)
            .map_err(|error| error.to_string())?;
        if out.len() <= budget {
            dropped.reverse();
            return Ok((out, dropped));
        }
        let Some(index) = ops
            .iter()
            .rposition(|op| matches!(op, packed::PackedOp::Update { .. }))
        else {
            // Structural ops alone exceed the budget: send them (correctness
            // beats the cap) and defer nothing.
            dropped.reverse();
            return Ok((out, dropped));
        };
        dropped.push(ops.remove(index));
    }
}

fn unreliable_bits(schema: &SchemaDef) -> u32 {
    let mut bits = 0u32;
    for (index, field) in schema.fields.iter().enumerate().take(32) {
        if field.flags & engine_schema::FIELD_UNRELIABLE != 0 {
            bits |= 1u32 << index;
        }
    }
    bits
}

fn strip_unreliable_ops(ops: &mut Vec<packed::PackedOp>, bits: u32) {
    for op in ops.iter_mut() {
        match op {
            packed::PackedOp::Add { values, .. } => {
                for index in 0..values.len().min(32) {
                    if bits & (1u32 << index) != 0 {
                        values[index] = packed::WireValue::Null;
                    }
                }
            }
            packed::PackedOp::Update { mask, values, .. } => {
                *mask &= !bits;
                values.retain(|(index, _)| bits & (1u32 << *index) == 0);
            }
            packed::PackedOp::Remove { .. } => {}
        }
    }
    ops.retain(|op| !matches!(op, packed::PackedOp::Update { mask: 0, .. }));
}

fn field_hidden(schema: &SchemaDef, index: usize, owner_visible: bool, hidden: u32) -> bool {
    (schema.fields[index].private() && !owner_visible) || hidden & (1u32 << index) != 0
}

fn row_owner(id: u32, owner_shift: Option<u32>) -> u32 {
    owner_shift.map(|shift| id >> shift).unwrap_or(id)
}

fn mask_rows(
    schema: &SchemaDef,
    rows: &mut Vec<(u32, Vec<packed::WireValue>)>,
    viewer: u32,
    owner_shift: Option<u32>,
    view: Option<&ViewMask>,
    hidden: u32,
) {
    let only_self = view.is_some_and(ViewMask::only_self);
    if only_self {
        rows.retain(|(id, _)| row_owner(*id, owner_shift) == viewer);
    }
    for (id, values) in rows {
        let owner_visible = row_owner(*id, owner_shift) == viewer;
        for index in 0..schema.fields.len() {
            if field_hidden(schema, index, owner_visible, hidden) {
                values[index] = packed::WireValue::Null;
            }
        }
    }
}

fn mask_ops(
    schema: &SchemaDef,
    ops: &mut Vec<packed::PackedOp>,
    viewer: u32,
    owner_shift: Option<u32>,
    view: Option<&ViewMask>,
    hidden: u32,
) {
    let only_self = view.is_some_and(ViewMask::only_self);
    if only_self {
        ops.retain(|op| match op {
            packed::PackedOp::Add { id, .. }
            | packed::PackedOp::Update { id, .. }
            | packed::PackedOp::Remove { id } => row_owner(*id, owner_shift) == viewer,
        });
    }
    for op in ops.iter_mut() {
        match op {
            packed::PackedOp::Add { id, values } => {
                let owner_visible = row_owner(*id, owner_shift) == viewer;
                for index in 0..schema.fields.len() {
                    if field_hidden(schema, index, owner_visible, hidden) {
                        values[index] = packed::WireValue::Null;
                    }
                }
            }
            packed::PackedOp::Update { id, mask, values } => {
                let owner_visible = row_owner(*id, owner_shift) == viewer;
                values.retain(|(index, _)| !field_hidden(schema, *index, owner_visible, hidden));
                if hidden != 0
                    || schema
                        .fields
                        .iter()
                        .any(engine_schema::SchemaField::private)
                {
                    for index in 0..schema.fields.len() {
                        if field_hidden(schema, index, owner_visible, hidden) {
                            *mask &= !(1u32 << index);
                        }
                    }
                }
            }
            packed::PackedOp::Remove { .. } => {}
        }
    }
    ops.retain(|op| !matches!(op, packed::PackedOp::Update { mask: 0, .. }));
}

fn mask_root(schema: &SchemaDef, values: &mut [packed::WireValue], viewer: u32, hidden: u32) {
    for index in 0..schema.fields.len() {
        if field_hidden(schema, index, false, hidden) {
            values[index] = packed::WireValue::Null;
        }
    }
    let _ = viewer;
}

fn mask_root_patch(
    schema: &SchemaDef,
    values: &mut Vec<(usize, packed::WireValue)>,
    mask: &mut u32,
    viewer: u32,
    hidden: u32,
) {
    values.retain(|(index, _)| !field_hidden(schema, *index, false, hidden));
    for index in 0..schema.fields.len() {
        if field_hidden(schema, index, false, hidden) {
            *mask &= !(1u32 << index);
        }
    }
    let _ = viewer;
}

#[cfg(test)]
mod tests;
