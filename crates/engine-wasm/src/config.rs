#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WasmConfig {
    pub optimize_for_speed: bool,

    pub enable_epoch_interruption: bool,

    pub epoch_deadline_ticks: Option<u64>,

    pub use_pooling_allocator: bool,
}
