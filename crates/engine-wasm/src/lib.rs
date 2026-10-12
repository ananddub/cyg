//! # Engine WASM
//!
//! A pure, generic WebAssembly runtime wrapper powered by Wasmtime.
//! Completely decoupled from game events, rooms, or any specific application logic.

pub mod config;
pub mod error;
pub mod instance;
pub mod macros;
pub mod runtime;

pub use config::WasmConfig;
pub use error::WasmError;
pub use instance::{WasmInstance, WasmPreInstance};
pub use runtime::WasmRuntime;
