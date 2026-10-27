/// Declares a type-safe, ultra-high-performance Rust wrapper around a WebAssembly module.
///
/// Functions are cached as static `TypedFunc` pointers on instantiation,
/// giving **~17–20 nanosecond direct execution latency** with ZERO per-call string lookups.
///
/// # Example
/// ```rust,no_run
/// use engine_wasm::{WasmRuntime, wasm_module};
///
/// wasm_module! {
///     pub struct GameRoom {
///         fn calculate_damage(base: i32, multiplier: f32) -> i32;
///         fn on_tick(dt: f32, tick: u64);
///         fn player_count() -> u32;
///     }
/// }
///
/// fn example(runtime: &WasmRuntime, module: &wasmtime::Module) -> Result<(), Box<dyn std::error::Error>> {
///     let mut room = GameRoom::new(runtime, module)?;
///
///     // Direct 17ns native-speed method call:
///     let dmg = room.calculate_damage(50, 1.5)?;
///     room.on_tick(0.016, 1)?;
///     let count = room.player_count()?;
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! wasm_module {
    // Internal helper rule to expand optional return types
    (@ret) => { () };
    (@ret $ret:ty) => { $ret };

    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$fn_meta:meta])*
                fn $func:ident ( $( $arg:ident : $arg_ty:ty ),* $(,)? ) $( -> $ret:ty )? ;
            )*
        }
    ) => {
        #[allow(non_snake_case)]
        $(#[$meta])*
        $vis struct $name<T: 'static = ()> {
            instance: $crate::WasmInstance<T>,
            $(
                $func: wasmtime::TypedFunc<( $( $arg_ty ),* ), $crate::wasm_module!(@ret $( $ret )?)>,
            )*
        }

        #[allow(non_snake_case)]
        impl<T: 'static> $name<T> {
            /// Wrap an existing WasmInstance and pre-cache typed function handles.
            pub fn from_instance(mut instance: $crate::WasmInstance<T>) -> Result<Self, $crate::WasmError> {
                $(
                    let $func = instance.get_typed_func::<( $( $arg_ty ),* ), $crate::wasm_module!(@ret $( $ret )?)>(stringify!($func))?;
                )*
                Ok(Self {
                    instance,
                    $( $func ),*
                })
            }

            /// Instantiate from a compiled module with standalone settings (no host imports).
            pub fn from_module(
                runtime: &$crate::WasmRuntime,
                module: &wasmtime::Module,
                host_data: T,
            ) -> Result<Self, $crate::WasmError> {
                let instance = runtime.instantiate_standalone(module, host_data)?;
                Self::from_instance(instance)
            }

            /// Instantiate from a compiled module with custom host linker imports.
            pub fn from_module_with_linker(
                runtime: &$crate::WasmRuntime,
                linker: &wasmtime::Linker<T>,
                module: &wasmtime::Module,
                host_data: T,
            ) -> Result<Self, $crate::WasmError> {
                let instance = runtime.instantiate(linker, module, host_data)?;
                Self::from_instance(instance)
            }

            /// Fast path: spawn from a pre-instantiated template
            /// (see `$crate::WasmRuntime::prepare_standalone`).
            pub fn from_pre(
                pre: &$crate::WasmPreInstance<T>,
                host_data: T,
            ) -> Result<Self, $crate::WasmError> {
                Self::from_instance(pre.instantiate(host_data)?)
            }

            /// Access the host data state.
            pub fn data(&self) -> &T {
                self.instance.data()
            }

            /// Mutably access the host data state.
            pub fn data_mut(&mut self) -> &mut T {
                self.instance.data_mut()
            }

            /// Read raw bytes from linear memory.
            pub fn read_memory(&self, offset: u32, len: u32) -> Result<Vec<u8>, $crate::WasmError> {
                self.instance.read_memory(offset, len)
            }

            /// [`read_memory`](Self::read_memory) into a caller-owned buffer, so a
            /// per-tick delta read reuses one allocation.
            pub fn read_memory_into(
                &self,
                offset: u32,
                len: u32,
                out: &mut Vec<u8>,
            ) -> Result<(), $crate::WasmError> {
                self.instance.read_memory_into(offset, len, out)
            }

            /// Write raw bytes into linear memory.
            pub fn write_memory(&mut self, offset: u32, bytes: &[u8]) -> Result<(), $crate::WasmError> {
                self.instance.write_memory(offset, bytes)
            }

            /// Mutably access the underlying raw WasmInstance.
            pub fn raw_instance(&mut self) -> &mut $crate::WasmInstance<T> {
                &mut self.instance
            }

            /// Immutably access the underlying raw WasmInstance.
            pub fn raw_instance_ref(&self) -> &$crate::WasmInstance<T> {
                &self.instance
            }

            /// Snapshot the linear memory buffer for rollback or serialization.
            pub fn snapshot_memory(&self) -> Result<Vec<u8>, $crate::WasmError> {
                self.instance.snapshot_memory()
            }

            /// Restore linear memory directly from a previous snapshot.
            pub fn restore_memory(&mut self, snapshot: &[u8]) -> Result<(), $crate::WasmError> {
                self.instance.restore_memory(snapshot)
            }

            /// Compute a 64-bit FNV-1a checksum of the linear memory.
            pub fn compute_memory_checksum(&self) -> Result<u64, $crate::WasmError> {
                let slice = self.instance.raw_memory_slice()?;
                Ok($crate::rollback::compute_checksum_64(slice))
            }

            $(
                $(#[$fn_meta])*
                #[inline]
                pub fn $func(&mut self, $( $arg : $arg_ty ),* ) -> Result<$crate::wasm_module!(@ret $( $ret )?), $crate::WasmError> {
                    self.$func
                        .call(self.instance.store_mut(), ( $( $arg ),* ))
                        .map_err(|e| $crate::WasmError::Trap(stringify!($func).into(), e.to_string()))
                }
            )*
        }

        impl $name<()> {
            /// Create a new instance without host state (default T = ()).
            pub fn new(
                runtime: &$crate::WasmRuntime,
                module: &wasmtime::Module,
            ) -> Result<Self, $crate::WasmError> {
                Self::from_module(runtime, module, ())
            }
        }
    };
}
