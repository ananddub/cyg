use thiserror::Error;

#[derive(Error, Debug)]
pub enum WasmError {
    #[error("Wasmtime error: {0}")]
    Engine(#[from] wasmtime::Error),

    #[error("Failed to compile WASM: {0}")]
    Compile(String),

    #[error("Instantiation failed: {0}")]
    Instantiation(String),

    #[error("Exported function '{name}' not found or signature mismatch: {reason}")]
    FunctionNotFound { name: String, reason: String },

    #[error("Exported memory 'memory' not found in WASM module")]
    MissingMemory,

    #[error("Memory access out of bounds: offset={offset}, len={len}, total_size={size}")]
    OutOfBounds {
        offset: usize,
        len: usize,
        size: usize,
    },

    #[error("Execution trap in function '{0}': {1}")]
    Trap(String, String),

    #[error("Module serialization/deserialization failed: {0}")]
    Serialization(String),
}
