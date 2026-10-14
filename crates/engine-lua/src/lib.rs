//! # OpenOxide Native Lua Guest Runtime (powered by mlua & LuaJIT)
//!
//! Provides ultra-high-speed, native-speed authoritative game room execution for Lua scripts.

use mlua::{Function, Lua, Table, Value};
use std::path::Path;
use thiserror::Error;

/// Built-in canonical OpenOxide Lua SDK module
pub const OPENOXIDE_LUA_SDK: &str = include_str!("../../../sdk/interfaces/lua/openoxide.lua");

#[derive(Debug, Error)]
pub enum LuaError {
    #[error("Lua script error: {0}")]
    Script(#[from] mlua::Error),
    #[error("IO error loading Lua script: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid room class: script must return a Table or Room instance")]
    InvalidRoomClass,
}

/// An authoritative OpenOxide Game Room powered by embedded LuaJIT.
pub struct LuaRoom {
    lua: Lua,
    room_instance: Table,
}

impl LuaRoom {
    /// Create a new Lua room from Lua script source code.
    pub fn from_source(source: &str) -> Result<Self, LuaError> {
        let lua = Lua::new();

        // 1. Preload `openoxide` module into package.preload so `require("openoxide")` succeeds natively
        let preload_sdk = lua.load(OPENOXIDE_LUA_SDK).into_function()?;
        let package: Table = lua.globals().get("package")?;
        let loaded: Table = package.get("loaded")?;
        let openoxide_mod = preload_sdk.call::<Table>(())?;
        loaded.set("openoxide", openoxide_mod)?;

        // 2. Load and evaluate the user room script
        let script_val: Value = lua.load(source).eval()?;
        let room_table: Table = match script_val {
            Value::Table(t) => t,
            _ => return Err(LuaError::InvalidRoomClass),
        };

        // If the table has a :new() method (class-based), instantiate it; otherwise use table directly
        let room_instance = if let Ok(new_fn) = room_table.get::<Function>("new") {
            new_fn.call::<Table>(room_table.clone())?
        } else {
            room_table
        };

        Ok(Self {
            lua,
            room_instance,
        })
    }

    /// Load a Lua room directly from a script file path.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, LuaError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_source(&content)
    }

    /// Called once when the room is allocated on the host.
    pub fn on_create(&self, max_clients: i32) -> Result<(), LuaError> {
        if let Ok(func) = self.room_instance.get::<Function>("onCreate") {
            func.call::<()>((self.room_instance.clone(), max_clients))?;
        }
        Ok(())
    }

    /// Room capacity check hook.
    pub fn on_is_full(&self, count: i32) -> Result<bool, LuaError> {
        if let Ok(func) = self.room_instance.get::<Function>("onIsFull") {
            let is_full: bool = func.call((self.room_instance.clone(), count))?;
            Ok(is_full)
        } else {
            Ok(false)
        }
    }

    /// Called when a new player joins with authoritative spawn coordinates.
    pub fn on_join(&self, id: i64, spawn_x: f64, spawn_z: f64) -> Result<(), LuaError> {
        if let Ok(func) = self.room_instance.get::<Function>("onJoin") {
            func.call::<()>((self.room_instance.clone(), id, spawn_x, spawn_z))?;
        }
        Ok(())
    }

    /// Called when a player leaves or disconnects.
    pub fn on_leave(&self, id: i64) -> Result<(), LuaError> {
        if let Ok(func) = self.room_instance.get::<Function>("onLeave") {
            func.call::<()>((self.room_instance.clone(), id))?;
        }
        Ok(())
    }

    /// Inbound client network packet input.
    pub fn on_input(&self, id: i64, data: &[u8]) -> Result<(), LuaError> {
        if let Ok(func) = self.room_instance.get::<Function>("onInput") {
            let lua_bytes = self.lua.create_string(data)?;
            func.call::<()>((self.room_instance.clone(), id, lua_bytes))?;
        }
        Ok(())
    }

    /// Fixed simulation tick (20 Hz - 128 Hz).
    /// Returns the raw binary delta byte buffer to broadcast.
    pub fn on_tick(&self, dt: f64) -> Result<Vec<u8>, LuaError> {
        if let Ok(func) = self.room_instance.get::<Function>("onTick") {
            let res: mlua::String = func.call((self.room_instance.clone(), dt))?;
            Ok(res.as_bytes().to_vec())
        } else {
            Ok(Vec::new())
        }
    }

    /// State persistence snapshot hook.
    pub fn on_save(&self) -> Result<Vec<u8>, LuaError> {
        if let Ok(func) = self.room_instance.get::<Function>("onSave") {
            let res: mlua::String = func.call(self.room_instance.clone())?;
            Ok(res.as_bytes().to_vec())
        } else {
            Ok(Vec::new())
        }
    }

    /// State restoration hook from saved linear snapshot.
    pub fn on_load(&self, data: &[u8]) -> Result<(), LuaError> {
        if let Ok(func) = self.room_instance.get::<Function>("onLoad") {
            let lua_bytes = self.lua.create_string(data)?;
            func.call::<()>((self.room_instance.clone(), lua_bytes))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_lua_room_lifecycle() {
        let lua_script = include_str!("../../../tests/guest_lua/room.lua");
        let room = LuaRoom::from_source(lua_script).expect("Lua room initialization ok");

        // 1. onCreate
        room.on_create(32).expect("on_create ok");

        // 2. onIsFull
        assert!(!room.on_is_full(10).expect("on_is_full ok"));
        assert!(room.on_is_full(32).expect("on_is_full ok"));

        // 3. onJoin
        room.on_join(1001, 15.0, 25.0).expect("on_join ok");

        // 4. onInput
        let input_packet = [0x01, 0x02, 0x03, 0x04];
        room.on_input(1001, &input_packet).expect("on_input ok");

        // 5. onTick
        let delta = room.on_tick(0.016).expect("on_tick ok");
        assert_eq!(delta.len(), 3);
        assert_eq!(delta[0], 1, "Player count must be 1 in delta");
        assert_eq!(delta[1], 0x77, "Lua magic marker must be 0x77");

        // 6. onSave
        let snapshot = room.on_save().expect("on_save ok");
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0], 0x33, "Snapshot magic marker must be 0x33");
        assert_eq!(snapshot[1], 1, "Snapshot player count must be 1");

        // 7. onLeave
        room.on_leave(1001).expect("on_leave ok");
        let delta_after_leave = room.on_tick(0.016).expect("on_tick after leave ok");
        assert_eq!(delta_after_leave[0], 0, "Player count must be 0 after leave");

        // 8. onLoad
        room.on_load(&snapshot).expect("on_load ok");
        let delta_restored = room.on_tick(0.016).expect("on_tick after restore ok");
        assert_eq!(delta_restored[0], 1, "Player count must be restored to 1");

        println!("🎉 Native LuaRoom mlua lifecycle test PASSED 100%!");
    }
}
