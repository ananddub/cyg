use bevy_ecs::prelude::*;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rand_seeder::Seeder;

/// Deterministic RNG seeded from any string (room ID, match seed, client seed).
/// Can be inserted as an ECS Resource in the room tick world.
#[derive(Resource, Debug, Clone)]
pub struct RoomRng {
    inner: ChaCha8Rng,
}

impl RoomRng {
    /// Creates a deterministic RNG instance from an arbitrary string seed.
    pub fn from_seed_str(seed: &str) -> Self {
        let inner: ChaCha8Rng = Seeder::from(seed).make_rng();
        Self { inner }
    }

    /// Creates an RNG directly from a numeric 64-bit seed.
    pub fn from_u64(seed: u64) -> Self {
        Self {
            inner: ChaCha8Rng::seed_from_u64(seed),
        }
    }

    /// Returns a pseudo-random floating point number in `[0.0, 1.0)`.
    pub fn next_f32(&mut self) -> f32 {
        self.inner.r#gen::<f32>()
    }

    /// Returns a pseudo-random 32-bit unsigned integer.
    pub fn next_u32(&mut self) -> u32 {
        self.inner.r#gen::<u32>()
    }

    /// Returns a pseudo-random integer within the range `[lo, hi)`.
    pub fn next_range(&mut self, lo: i32, hi: i32) -> i32 {
        self.inner.gen_range(lo..hi)
    }

    /// Returns true or false with given probability (e.g. 0.5 for 50/50 coin flip).
    pub fn next_bool(&mut self, p: f64) -> bool {
        self.inner.gen_bool(p)
    }
}
