//! =============================================================================
//! CYG Game Server — Guest Room SDK for Rust
//! WebAssembly Target: wasm32-unknown-unknown / wasm32-wasi
//! =============================================================================

/// Trait defining the lifecycle of an authoritative CYG game room.
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
        static mut G_CYG_ROOM: Option<$RoomType> = None;

        #[no_mangle]
        pub extern "C" fn on_create(max: i32) {
            unsafe {
                let mut room = $RoomType::default();
                room.on_create(max);
                G_CYG_ROOM = Some(room);
            }
        }

        #[no_mangle]
        pub extern "C" fn on_join(id: i64, spawn_x: f64, spawn_z: f64) {
            unsafe {
                if let Some(ref mut room) = G_CYG_ROOM {
                    room.on_join(id, spawn_x, spawn_z);
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn on_leave(id: i64) {
            unsafe {
                if let Some(ref mut room) = G_CYG_ROOM {
                    room.on_leave(id);
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn on_input(id: i64, ptr: i32, len: i32) {
            unsafe {
                if let Some(ref mut room) = G_CYG_ROOM {
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
                if let Some(ref mut room) = G_CYG_ROOM {
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
                if let Some(ref mut room) = G_CYG_ROOM {
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
                if let Some(ref mut room) = G_CYG_ROOM {
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
                if let Some(ref mut room) = G_CYG_ROOM {
                    room.on_alloc(len as usize) as i32
                } else {
                    0
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn on_is_full(count: i32) -> bool {
            unsafe {
                if let Some(ref mut room) = G_CYG_ROOM {
                    room.on_is_full(count)
                } else {
                    false
                }
            }
        }
    };
}

/// Universal Hardware KeyCodes (A-Z, 0-9, Arrows, F1-F12, Mouse Buttons)
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    None = 0,
    // Mouse
    MouseLeft = 1, MouseMiddle = 2, MouseRight = 3, MouseBack = 4, MouseForward = 5,
    // Control & Whitespace
    Backspace = 8, Tab = 9, Enter = 13, Shift = 16, Control = 17, Alt = 18,
    Pause = 19, CapsLock = 20, Escape = 27, Space = 32,
    // Navigation
    PageUp = 33, PageDown = 34, End = 35, Home = 36, Insert = 45, Delete = 46, PrintScreen = 44,
    // Arrows
    ArrowLeft = 37, ArrowUp = 38, ArrowRight = 39, ArrowDown = 40,
    // Digits
    Digit0 = 48, Digit1 = 49, Digit2 = 50, Digit3 = 51, Digit4 = 52,
    Digit5 = 53, Digit6 = 54, Digit7 = 55, Digit8 = 56, Digit9 = 57,
    // Alphabet A-Z
    KeyA = 65, KeyB = 66, KeyC = 67, KeyD = 68, KeyE = 69, KeyF = 70,
    KeyG = 71, KeyH = 72, KeyI = 73, KeyJ = 74, KeyK = 75, KeyL = 76,
    KeyM = 77, KeyN = 78, KeyO = 79, KeyP = 80, KeyQ = 81, KeyR = 82,
    KeyS = 83, KeyT = 84, KeyU = 85, KeyV = 86, KeyW = 87, KeyX = 88,
    KeyY = 89, KeyZ = 90,
    // Meta
    MetaLeft = 91, MetaRight = 92, ContextMenu = 93,
    // Numpad
    Numpad0 = 96, Numpad1 = 97, Numpad2 = 98, Numpad3 = 99, Numpad4 = 100,
    Numpad5 = 101, Numpad6 = 102, Numpad7 = 103, Numpad8 = 104, Numpad9 = 105,
    NumpadMultiply = 106, NumpadAdd = 107, NumpadSeparator = 108,
    NumpadSubtract = 109, NumpadDecimal = 110, NumpadDivide = 111,
    // Function Keys
    F1 = 112, F2 = 113, F3 = 114, F4 = 115, F5 = 116, F6 = 117,
    F7 = 118, F8 = 119, F9 = 120, F10 = 121, F11 = 122, F12 = 123,
    // Modifiers Specific
    ShiftLeft = 160, ShiftRight = 161, ControlLeft = 162, ControlRight = 163,
    AltLeft = 164, AltRight = 165,
}

