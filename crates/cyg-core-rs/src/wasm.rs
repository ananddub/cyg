//! Web bindings: `cargo build --target wasm32-unknown-unknown --features wasm`
//! produces `cyg_core_rs_bg.wasm` (wasm-bindgen) that cyg.js loads as its
//! decode core. Same semantics as the C ABI.

use wasm_bindgen::prelude::*;
#[allow(unused_imports)]
use js_sys as _js_sys;

/// Web-exposed client core. JS keeps the WebSocket; every frame's bytes go
/// into [`apply_payload`], and typed values are read back with `get_f64`.
#[wasm_bindgen]
pub struct WasmClientCore {
    inner: crate::ClientCore,
}

#[wasm_bindgen]
impl WasmClientCore {
    /// Parse `cyg.schema.bin` bytes. Throws on a bad descriptor.
    #[wasm_bindgen(constructor)]
    pub fn new(descriptor: &[u8]) -> Result<WasmClientCore, wasm_bindgen::JsValue> {
        crate::ClientCore::new(descriptor)
            .map(|inner| WasmClientCore { inner })
            .map_err(|e| wasm_bindgen::JsValue::from_str(&e))
    }

    /// Apply one raw room-data payload; returns the number of events.
    #[wasm_bindgen]
    pub fn apply_payload(&mut self, payload: &[u8]) -> Result<usize, wasm_bindgen::JsValue> {
        self.inner.apply_payload(payload)
            .map(|events| events.len())
            .map_err(|e| wasm_bindgen::JsValue::from_str(&e))
    }

    /// f64 view of one entity field by schema-field INDEX.
    #[wasm_bindgen]
    pub fn get_f64(&self, entity_id: u32, field_index: usize) -> f64 {
        let state = self.inner.state();
        let Some(coll) = state.collections.first() else { return f64::NAN };
        let Some(entity) = coll.entities.get(&entity_id) else { return f64::NAN };
        entity.values.get(field_index).map(|v| v.as_f64()).unwrap_or(f64::NAN)
    }

    /// Entity count in the primary collection.
    #[wasm_bindgen]
    pub fn entity_count(&self) -> u32 {
        self.inner.state().collections.first()
            .map(|c| c.entities.len() as u32)
            .unwrap_or(0)
    }

    /// Is the client ready (kind-1 full snapshot applied)?
    #[wasm_bindgen]
    pub fn ready(&self) -> bool {
        self.inner.state().ready()
    }
}
