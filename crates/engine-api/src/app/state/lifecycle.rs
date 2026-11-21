//! Process-wide handle to the `engine-manager` lifecycle.
//!
//! The control plane is started before the host loop finishes booting, so it
//! must be able to ask "is the server actually serving yet?" without owning the
//! host. `engine-control` installs the same `ServerManager` the host drives;
//! the matchmake and game-socket routes use it to fail fast while booting or
//! draining. Absent (tests, embedded callers) means no gate.

use std::sync::{Arc, OnceLock};

use engine_manager::ServerManager;

static LIFECYCLE: OnceLock<Arc<ServerManager>> = OnceLock::new();

/// Install the process lifecycle. First install wins; later boots (tests,
/// embedded servers) stay ungated rather than replacing a live manager.
pub fn install(manager: Arc<ServerManager>) {
    let _ = LIFECYCLE.set(manager);
}

/// The installed lifecycle, if this process has one.
pub fn manager() -> Option<&'static Arc<ServerManager>> {
    LIFECYCLE.get()
}
