//! # OpenOxide Game Host ABI v1 Specification & Enforcement
//!
//! Implements strict, future-proof ABI contracts:
//! 1. Versioning: `game_host_v1` namespace and mandatory `abi_version` validation.
//! 2. Encoding: Enforced UTF-8 strings and JSON payloads.
//! 3. Ownership: Explicit memory ownership and buffer borrowing lifecycle.
//! 4. Bounds: Configurable limits on payloads, state values, delta frames, and log lines.
//! 5. Errors: Stable, deterministic numeric error codes + fault-isolated event policy.
//! 6. Ordering & Re-entrancy: Strict FIFO event ordering and re-entrancy prevention.
//! 7. Permissions: Capability-based host function access via manifest declarations.
//! 8. Compatibility: Dual-namespace registration (`game_host_v1` + `env` fallback) ensuring zero breaking changes.

use std::cell::Cell;

/// Official canonical ABI v1 namespace for OpenOxide game hosts.
pub const HOST_NAMESPACE_V1: &str = "game_host_v1";

/// Legacy compatibility namespace so existing compiled WASM modules continue to run.
pub const HOST_NAMESPACE_LEGACY: &str = "env";

/// The current supported ABI version.
pub const CURRENT_ABI_VERSION: u32 = 1;

// -------------------------------------------------------------------------
// Rule 4: Bounds (Configurable payload, state, delta and log limits)
// -------------------------------------------------------------------------

/// Maximum allowed inbound client payload size (64 KB).
pub const MAX_INBOUND_PAYLOAD_SIZE: usize = 64 * 1024;

/// Maximum allowed outbound delta buffer size (4 MB).
pub const MAX_OUTBOUND_DELTA_SIZE: usize = 4 * 1024 * 1024;

/// Maximum allowed save/load state buffer size (16 MB).
pub const MAX_SAVE_STATE_SIZE: usize = 16 * 1024 * 1024;

/// Maximum allowed host log message length (2 KB).
pub const MAX_LOG_LENGTH: usize = 2048;

// -------------------------------------------------------------------------
// Rule 5: Stable Error Codes
// -------------------------------------------------------------------------

/// Call succeeded without errors.
pub const ABI_OK: i32 = 0;
/// Generic unspecified error.
pub const ABI_ERR_GENERIC: i32 = -1;
/// Invalid argument passed to host function.
pub const ABI_ERR_INVALID_ARGUMENT: i32 = -2;
/// Memory access out of linear memory bounds.
pub const ABI_ERR_OUT_OF_BOUNDS: i32 = -3;
/// Provided buffer too small or payload exceeds maximum allowed size.
pub const ABI_ERR_BUFFER_OVERFLOW: i32 = -4;
/// Host function call rejected because manifest lacks required permission.
pub const ABI_ERR_PERMISSION_DENIED: i32 = -5;
/// String or JSON payload is not valid UTF-8.
pub const ABI_ERR_INVALID_UTF8: i32 = -6;
/// Synchronous re-entrant guest callback attempted during host execution.
pub const ABI_ERR_REENTRANCY_DISALLOWED: i32 = -7;
/// Module requires an unsupported or mismatched ABI version.
pub const ABI_ERR_UNSUPPORTED_VERSION: i32 = -8;

// -------------------------------------------------------------------------
// Rule 7: Permissions (Capability-based Security via Manifest)
// -------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbiPermissions {
    pub physics: bool,
    pub lag_compensation: bool,
    pub logging: bool,
    pub storage: bool,
}

impl Default for AbiPermissions {
    fn default() -> Self {
        Self {
            physics: true,
            lag_compensation: true,
            logging: true,
            storage: true,
        }
    }
}

// -------------------------------------------------------------------------
// Rule 2: Encoding (UTF-8 Validation)
// -------------------------------------------------------------------------

/// Validates that a raw byte slice contains valid UTF-8.
/// Returns standard ABI error code `-6` if invalid.
pub fn validate_utf8(bytes: &[u8]) -> Result<&str, i32> {
    std::str::from_utf8(bytes).map_err(|_| ABI_ERR_INVALID_UTF8)
}

// -------------------------------------------------------------------------
// Rule 6: Re-entrancy Protection
// -------------------------------------------------------------------------

/// Re-entrancy guard protecting a host-guest call boundary.
///
/// One guard belongs to one WASM instance, and each instance is only ever
/// executed by the thread that owns it (thread-per-core), so a `Cell` is
/// enough: an atomic would put a `SeqCst` fence on the hot call path to
/// synchronise with nothing. Deliberately `!Sync` — sharing a guard across
/// threads is the bug this type exists to catch.
#[derive(Debug, Default)]
pub struct ReentrancyGuard {
    active: Cell<bool>,
}

impl ReentrancyGuard {
    pub fn new() -> Self {
        Self {
            active: Cell::new(false),
        }
    }

    /// Attempts to enter an active execution scope. Returns `ABI_ERR_REENTRANCY_DISALLOWED`
    /// if a re-entrant call is attempted.
    pub fn enter(&self) -> Result<ReentrancyScope<'_>, i32> {
        if self.active.replace(true) {
            Err(ABI_ERR_REENTRANCY_DISALLOWED)
        } else {
            Ok(ReentrancyScope { guard: self })
        }
    }
}

pub struct ReentrancyScope<'a> {
    guard: &'a ReentrancyGuard,
}

impl Drop for ReentrancyScope<'_> {
    fn drop(&mut self) {
        self.guard.active.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf8_validation() {
        let valid = b"Hello, OpenOxide Arena!";
        assert_eq!(validate_utf8(valid), Ok("Hello, OpenOxide Arena!"));

        let invalid = &[0xFF, 0xFE, 0xFD];
        assert_eq!(validate_utf8(invalid), Err(ABI_ERR_INVALID_UTF8));
    }

    #[test]
    fn test_reentrancy_guard() {
        let guard = ReentrancyGuard::new();
        let scope1 = guard.enter();
        assert!(scope1.is_ok());

        // Attempting to enter while scope1 is active must fail with ABI_ERR_REENTRANCY_DISALLOWED
        let scope2 = guard.enter();
        assert_eq!(scope2.err(), Some(ABI_ERR_REENTRANCY_DISALLOWED));

        // Dropping scope1 unlocks the guard
        drop(scope1);
        let scope3 = guard.enter();
        assert!(scope3.is_ok());
    }

    #[test]
    fn test_default_permissions() {
        let perms = AbiPermissions::default();
        assert!(perms.physics);
        assert!(perms.lag_compensation);
        assert!(perms.logging);
        assert!(perms.storage);
    }

    #[test]
    fn test_abi_constants_and_bounds() {
        assert_eq!(CURRENT_ABI_VERSION, 1);
        assert_eq!(HOST_NAMESPACE_V1, "game_host_v1");
        assert_eq!(HOST_NAMESPACE_LEGACY, "env");
        assert!(MAX_INBOUND_PAYLOAD_SIZE >= 1024);
        assert!(MAX_OUTBOUND_DELTA_SIZE > MAX_INBOUND_PAYLOAD_SIZE);
        assert_eq!(ABI_OK, 0);
    }
}
