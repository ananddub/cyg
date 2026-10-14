//! =============================================================================
//! OpenOxide Game Server — Guest Room SDK for Rust
//! WebAssembly Target: wasm32-unknown-unknown / wasm32-wasi
//! =============================================================================

/// Trait defining the lifecycle of an authoritative OpenOxide game room.
pub trait Room: Default {
    /// Called once when the room instance is allocated on the host.
    fn on_create(&mut self, max_clients: i32);

    /// Called when a new player joins the room.
    fn on_join(&mut self, id: i64, spawn_x: f64, spawn_z: f64);

    /// Called when a player disconnects or leaves.
    fn on_leave(&mut self, id: i64);

    /// Called when an authoritative client input packet arrives.
    fn on_input(&mut self, id: i64, data: &[u8]);

    /// Fixed authoritative simulation tick (e.g. 20 Hz, 60 Hz, 128 Hz).
    /// Returns (buffer_pointer, buffer_length) in WASM linear memory.
    fn on_tick(&mut self, dt: f64) -> (usize, usize);

    /// Rollback/State persistence save hook (optional).
    fn on_save(&mut self) -> (usize, usize) {
        (0, 0)
    }

    /// Rollback/State persistence load hook (optional).
    fn on_load(&mut self, _data: &[u8]) {}

    /// Inbound guest memory allocator for network packets.
    fn on_alloc(&mut self, len: usize) -> *mut u8 {
        let mut buf = Vec::with_capacity(len);
        let ptr = buf.as_mut_ptr();
        std::mem::forget(buf);
        ptr
    }

    /// Room capacity check hook.
    fn on_is_full(&mut self, count: i32) -> bool {
        let _ = count;
        false
    }
}

/// Helper to pack linear memory (ptr, len) into a 64-bit integer.
#[inline(always)]
pub fn pack_delta(ptr: usize, len: usize) -> i64 {
    ((ptr as i64) << 32) | (len as i64 & 0xFFFFFFFF)
}

/// Macro to export WebAssembly C-ABI functions for the room type automatically.
#[macro_export]
macro_rules! export_room {
    ($RoomType:ident) => {
        static mut G_OPENOXIDE_ROOM: Option<$RoomType> = None;

        #[no_mangle]
        pub extern "C" fn on_create(max: i32) {
            unsafe {
                let mut room = $RoomType::default();
                room.on_create(max);
                G_OPENOXIDE_ROOM = Some(room);
            }
        }

        #[no_mangle]
        pub extern "C" fn on_join(id: i64, spawn_x: f64, spawn_z: f64) {
            unsafe {
                if let Some(ref mut room) = G_OPENOXIDE_ROOM {
                    room.on_join(id, spawn_x, spawn_z);
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn on_leave(id: i64) {
            unsafe {
                if let Some(ref mut room) = G_OPENOXIDE_ROOM {
                    room.on_leave(id);
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn on_input(id: i64, ptr: i32, len: i32) {
            unsafe {
                if let Some(ref mut room) = G_OPENOXIDE_ROOM {
                    if ptr != 0 && len > 0 {
                        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
                        room.on_input(id, slice);
                    }
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn on_tick(dt: f64) -> i64 {
            unsafe {
                if let Some(ref mut room) = G_OPENOXIDE_ROOM {
                    let (ptr, len) = room.on_tick(dt);
                    pack_delta(ptr, len)
                } else {
                    0
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn on_save() -> i64 {
            unsafe {
                if let Some(ref mut room) = G_OPENOXIDE_ROOM {
                    let (ptr, len) = room.on_save();
                    pack_delta(ptr, len)
                } else {
                    0
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn on_load(ptr: i32, len: i32) {
            unsafe {
                if let Some(ref mut room) = G_OPENOXIDE_ROOM {
                    if ptr != 0 && len > 0 {
                        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
                        room.on_load(slice);
                    }
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn on_alloc(len: i32) -> i32 {
            unsafe {
                if let Some(ref mut room) = G_OPENOXIDE_ROOM {
                    room.on_alloc(len as usize) as i32
                } else {
                    0
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn on_is_full(count: i32) -> bool {
            unsafe {
                if let Some(ref mut room) = G_OPENOXIDE_ROOM {
                    room.on_is_full(count)
                } else {
                    false
                }
            }
        }
    };
}
