use std::cell::Cell;

pub const HOST_NAMESPACE_V1: &str = "game_host_v1";

pub const HOST_NAMESPACE_LEGACY: &str = "env";

pub const CURRENT_ABI_VERSION: u32 = 1;

pub const MAX_INBOUND_PAYLOAD_SIZE: usize = 64 * 1024;

pub const MAX_OUTBOUND_DELTA_SIZE: usize = 4 * 1024 * 1024;

pub const MAX_SAVE_STATE_SIZE: usize = 16 * 1024 * 1024;

pub const MAX_LOG_LENGTH: usize = 2048;

pub const ABI_OK: i32 = 0;

pub const ABI_ERR_GENERIC: i32 = -1;

pub const ABI_ERR_INVALID_ARGUMENT: i32 = -2;

pub const ABI_ERR_OUT_OF_BOUNDS: i32 = -3;

pub const ABI_ERR_BUFFER_OVERFLOW: i32 = -4;

pub const ABI_ERR_PERMISSION_DENIED: i32 = -5;

pub const ABI_ERR_INVALID_UTF8: i32 = -6;

pub const ABI_ERR_REENTRANCY_DISALLOWED: i32 = -7;

pub const ABI_ERR_UNSUPPORTED_VERSION: i32 = -8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbiPermissions {
    pub physics: bool,
    pub lag_compensation: bool,
    pub logging: bool,
    pub storage: bool,

    pub messaging: bool,
}

impl Default for AbiPermissions {
    fn default() -> Self {
        Self {
            physics: true,
            lag_compensation: true,
            logging: true,
            storage: true,
            messaging: true,
        }
    }
}

pub fn validate_utf8(bytes: &[u8]) -> Result<&str, i32> {
    std::str::from_utf8(bytes).map_err(|_| ABI_ERR_INVALID_UTF8)
}

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
        let valid = b"Hello, CYG Arena!";
        assert_eq!(validate_utf8(valid), Ok("Hello, CYG Arena!"));

        let invalid = &[0xFF, 0xFE, 0xFD];
        assert_eq!(validate_utf8(invalid), Err(ABI_ERR_INVALID_UTF8));
    }

    #[test]
    fn test_reentrancy_guard() {
        let guard = ReentrancyGuard::new();
        let scope1 = guard.enter();
        assert!(scope1.is_ok());

        let scope2 = guard.enter();
        assert_eq!(scope2.err(), Some(ABI_ERR_REENTRANCY_DISALLOWED));

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
