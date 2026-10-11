//! # Engine Guest SDK
//!
//! Helper macros and bindings for guest WebAssembly game rooms.
//! Developers compile their game logic to `.wasm` using this crate.

pub trait GuestRoom {
    fn on_init(&mut self);
    fn on_tick(&mut self, dt: f32);
    fn on_player_join(&mut self, player_id: u32);
    fn on_player_leave(&mut self, player_id: u32);
}
