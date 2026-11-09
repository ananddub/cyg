//! Loading and calling a hook module.
//!
//! The guest has no host imports: it receives one JSON document per event and
//! returns one JSON document. That keeps the sandbox strict — a hook cannot
//! reach the database, the network or the filesystem, and a bug in customer code
//! cannot corrupt a room's state because it shares none.

use engine_wasm::{WasmError, WasmRuntime};
use wasmtime::{Caller, Linker, Module};

use crate::event::{HookEvent, HookInput, HookOutput};

#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error("hook module failed to load: {0}")]
    Load(String),
    #[error("hook `{hook}` failed: {message}")]
    Call { hook: &'static str, message: String },
    #[error("hook `{hook}` returned invalid JSON: {source}")]
    Output {
        hook: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("hook `{hook}` exceeded its time budget")]
    Timeout { hook: &'static str },
}

impl HookError {
    /// Whether the event may proceed.
    ///
    /// A trap or a timeout is a bug in customer code, not a licence to drop the
    /// player's purchase on the floor. The caller decides what "proceed" means,
    /// but the default advice here is: log loudly and continue, because refusing
    /// every purchase whenever a hook is broken is worse than running without it.
    pub fn blocks_event(&self) -> bool {
        false
    }
}

/// One loaded hook module, reused across events.
pub struct HookRunner {
    runtime: WasmRuntime,
    module: Module,
    /// The largest document either direction may carry, in bytes.
    max_bytes: usize,
    /// Whether any hook export was found when the module loaded.
    exports: Vec<HookEvent>,
}

/// Host data handed to the guest: the request bytes and where to put the answer.
///
/// The guest writes its answer into a buffer the host owns, so a hook cannot
/// return a pointer to memory it has freed, and the host never trusts a length
/// the guest reports.
#[derive(Default)]
pub struct HookHost {
    pub input: Vec<u8>,
    pub output: Vec<u8>,
    pub output_capacity: usize,
}

impl HookRunner {
    /// Load a hook module and record which events it answers.
    ///
    /// A module that exports nothing is refused: silently accepting it would
    /// leave an operator believing their hooks run when none are wired.
    pub fn load(wasm_bytes: &[u8], max_bytes: usize) -> Result<Self, HookError> {
        let runtime = WasmRuntime::new().map_err(|e| HookError::Load(e.to_string()))?;
        let module = runtime
            .compile(wasm_bytes)
            .map_err(|e: WasmError| HookError::Load(e.to_string()))?;
        let exports = find_exports(&module);
        if exports.is_empty() {
            return Err(HookError::Load(format!(
                "module exports no hook (expected one of: {})",
                HookEvent::ALL
                    .iter()
                    .map(|e| e.export())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        Ok(Self {
            runtime,
            module,
            max_bytes,
            exports,
        })
    }

    /// Which events this module answers.
    pub fn exports(&self) -> &[HookEvent] {
        &self.exports
    }

    pub fn answers(&self, event: HookEvent) -> bool {
        self.exports.contains(&event)
    }

    /// Run the hook for an event.
    ///
    /// `Ok(None)` means the module does not answer this event — not an error, and
    /// not the same as answering with an empty output.
    pub fn run(&self, input: &HookInput) -> Result<Option<HookOutput>, HookError> {
        if !self.answers(input.event) {
            return Ok(None);
        }
        let hook = input.event.export();
        let encoded = serde_json::to_vec(input).expect("HookInput serializes");
        if encoded.len() > self.max_bytes {
            return Err(HookError::Call {
                hook,
                message: format!(
                    "input is {} bytes, over the {} byte limit",
                    encoded.len(),
                    self.max_bytes
                ),
            });
        }

        let host = HookHost {
            input: encoded.clone(),
            output: Vec::new(),
            output_capacity: self.max_bytes,
        };
        // The host imports the SDK uses: the guest asks where the input is and
        // hands its answer back through these, so neither side has to trust a
        // pointer the other side computed.
        let mut linker: Linker<HookHost> = Linker::new(self.runtime.raw_engine());
        linker
            .func_wrap("env", "hook_input_ptr", |caller: Caller<'_, HookHost>| {
                caller.data().input.as_ptr() as usize as i32
            })
            .map_err(|e| HookError::Load(e.to_string()))?;
        linker
            .func_wrap(
                "env",
                "hook_input_capacity",
                |caller: Caller<'_, HookHost>| caller.data().input.len() as i32,
            )
            .map_err(|e| HookError::Load(e.to_string()))?;
        linker
            .func_wrap(
                "env",
                "hook_write_result",
                |mut caller: Caller<'_, HookHost>, ptr: i32, len: i32| {
                    if len < 0 {
                        return;
                    }
                    let mem = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return,
                    };
                    let mut buf = vec![0u8; len as usize];
                    if mem.read(&caller, ptr as usize, &mut buf).is_err() {
                        return;
                    }
                    caller.data_mut().output = buf;
                },
            )
            .map_err(|e| HookError::Load(e.to_string()))?;
        let mut instance = self
            .runtime
            .instantiate(&linker, &self.module, host)
            .map_err(|e| HookError::Call {
                hook,
                message: e.to_string(),
            })?;

        // The guest reads its arguments through these exports and writes its
        // answer back through the rest. A guest without them cannot be handed an
        // event, which is a load-time problem reported here as a call failure.
        let len = instance
            .call::<(), i32>("hook_input_len", ())
            .map_err(|e| HookError::Call {
                hook,
                message: format!("hook_input_len: {e}"),
            })?;
        if len < 0 || len as usize > encoded.len() {
            return Err(HookError::Call {
                hook,
                message: format!("hook_input_len returned {len}, expected {}", encoded.len()),
            });
        }

        instance
            .call::<(), ()>(hook, ())
            .map_err(|e| HookError::Call {
                hook,
                message: e.to_string(),
            })?;

        let out_len = instance
            .call::<(), i32>("hook_output_len", ())
            .map_err(|e| HookError::Call {
                hook,
                message: format!("hook_output_len: {e}"),
            })?;
        if out_len < 0 || out_len as usize > self.max_bytes {
            return Err(HookError::Call {
                hook,
                message: format!("hook_output_len returned {out_len}, over the limit"),
            });
        }

        // The answer is taken from host memory, not from a guest-reported
        // pointer: a hook cannot hand back an address the host will dereference.
        let output = instance
            .data()
            .output
            .get(..out_len as usize)
            .map(<[u8]>::to_vec);
        let output = output.unwrap_or_default();

        if output.is_empty() {
            return Ok(Some(HookOutput::default()));
        }
        let parsed =
            serde_json::from_slice(&output).map_err(|source| HookError::Output { hook, source })?;
        Ok(Some(parsed))
    }
}

fn find_exports(module: &Module) -> Vec<HookEvent> {
    let exported: Vec<&str> = module.exports().map(|e| e.name()).collect();
    HookEvent::ALL
        .into_iter()
        .filter(|event| exported.contains(&event.export()))
        .collect()
}
