/// ABI constants and the guest permission set.
///
/// Lives in `engine-config` — the crate every other crate already depends on —
/// because both `engine-config` (per-game permissions in config.toml) and
/// `engine-wasm` (the sandbox enforcing them) need it. It used to live in
/// `engine-wasm`, which dragged wasmtime into the dependency graph of every
/// config consumer.

pub const CURRENT_ABI_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbiPermissions {
    pub physics: bool,
    pub lag_compensation: bool,
    pub logging: bool,
    pub storage: bool,

    pub messaging: bool,

    /// Guest may credit per-player counters. The host validates the seat, the
    /// stat name and the delta, coalesces per tick, and the control plane is
    /// the only writer that reaches the database.
    pub stats: bool,

    /// Guest may narrow a player's state view (`host_view_hide`,
    /// `host_view_only_self`, `host_view_reset`). Views can only subtract
    /// fields/rows; `.private()` still wins over every view.
    pub views: bool,

    /// Guest may record and query generic per-entity rewind samples.
    pub rewind: bool,
}

/// Deny by default: a guest only gets a host capability once the game's
/// config.toml asks for it (`allow_physics`, `allow_storage`, …). Anything a
/// future config field forgets to set stays closed instead of silently open.
impl Default for AbiPermissions {
    fn default() -> Self {
        Self {
            physics: false,
            lag_compensation: false,
            logging: false,
            storage: false,
            messaging: false,
            stats: false,
            views: false,
            rewind: false,
        }
    }
}
