use bevy_ecs::prelude::*;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rand_seeder::Seeder;

#[derive(Resource, Debug, Clone)]
pub struct RoomRng {
    inner: ChaCha8Rng,
}

impl RoomRng {

    pub fn from_seed_str(seed: &str) -> Self {
        let inner: ChaCha8Rng = Seeder::from(seed).make_rng();
        Self { inner }
    }

    pub fn from_u64(seed: u64) -> Self {
        Self {
            inner: ChaCha8Rng::seed_from_u64(seed),
        }
    }

    pub fn next_f32(&mut self) -> f32 {
        self.inner.r#gen::<f32>()
    }

    pub fn next_u32(&mut self) -> u32 {
        self.inner.r#gen::<u32>()
    }

    pub fn next_range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        self.inner.gen_range(lo..hi)
    }

    pub fn next_bool(&mut self, p: f64) -> bool {

        self.inner.gen_bool(p.max(0.0).min(1.0))
    }
}
