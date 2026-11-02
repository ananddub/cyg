
#[macro_export]
macro_rules! wasm_module {

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

            pub fn from_instance(mut instance: $crate::WasmInstance<T>) -> Result<Self, $crate::WasmError> {
                $(
                    let $func = instance.get_typed_func::<( $( $arg_ty ),* ), $crate::wasm_module!(@ret $( $ret )?)>(stringify!($func))?;
                )*
                Ok(Self {
                    instance,
                    $( $func ),*
                })
            }

            pub fn from_module(
                runtime: &$crate::WasmRuntime,
                module: &wasmtime::Module,
                host_data: T,
            ) -> Result<Self, $crate::WasmError> {
                let instance = runtime.instantiate_standalone(module, host_data)?;
                Self::from_instance(instance)
            }

            pub fn from_module_with_linker(
                runtime: &$crate::WasmRuntime,
                linker: &wasmtime::Linker<T>,
                module: &wasmtime::Module,
                host_data: T,
            ) -> Result<Self, $crate::WasmError> {
                let instance = runtime.instantiate(linker, module, host_data)?;
                Self::from_instance(instance)
            }

            pub fn from_pre(
                pre: &$crate::WasmPreInstance<T>,
                host_data: T,
            ) -> Result<Self, $crate::WasmError> {
                Self::from_instance(pre.instantiate(host_data)?)
            }

            pub fn data(&self) -> &T {
                self.instance.data()
            }

            pub fn data_mut(&mut self) -> &mut T {
                self.instance.data_mut()
            }

            pub fn read_memory(&self, offset: u32, len: u32) -> Result<Vec<u8>, $crate::WasmError> {
                self.instance.read_memory(offset, len)
            }

            pub fn read_memory_into(
                &self,
                offset: u32,
                len: u32,
                out: &mut Vec<u8>,
            ) -> Result<(), $crate::WasmError> {
                self.instance.read_memory_into(offset, len, out)
            }

            pub fn write_memory(&mut self, offset: u32, bytes: &[u8]) -> Result<(), $crate::WasmError> {
                self.instance.write_memory(offset, bytes)
            }

            pub fn raw_instance(&mut self) -> &mut $crate::WasmInstance<T> {
                &mut self.instance
            }

            pub fn raw_instance_ref(&self) -> &$crate::WasmInstance<T> {
                &self.instance
            }

            pub fn snapshot_memory(&self) -> Result<Vec<u8>, $crate::WasmError> {
                self.instance.snapshot_memory()
            }

            pub fn restore_memory(&mut self, snapshot: &[u8]) -> Result<(), $crate::WasmError> {
                self.instance.restore_memory(snapshot)
            }

            pub fn compute_memory_checksum(&self) -> Result<u64, $crate::WasmError> {
                let slice = self.instance.raw_memory_slice()?;
                Ok($crate::rollback::compute_checksum_64(slice))
            }

            $(
                $(#[$fn_meta])*
                #[inline]
                pub fn $func(&mut self, $( $arg : $arg_ty ),* ) -> Result<$crate::wasm_module!(@ret $( $ret )?), $crate::WasmError> {
                    self.instance.rearm_epoch_call();
                    self.$func
                        .call(self.instance.store_mut(), ( $( $arg ),* ))
                        .map_err(|e| {
                            $crate::WasmError::Trap(
                                stringify!($func).into(),
                                $crate::trap_detail(&e),
                            )
                        })
                }
            )*
        }

        impl $name<()> {

            pub fn new(
                runtime: &$crate::WasmRuntime,
                module: &wasmtime::Module,
            ) -> Result<Self, $crate::WasmError> {
                Self::from_module(runtime, module, ())
            }
        }
    };
}
