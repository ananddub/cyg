mod guard;
mod mime;
mod paths;
mod read_cache;

use std::path::{Path, PathBuf};

use super::StaticServer;

pub(super) fn server(root: &Path) -> StaticServer {
    StaticServer::new(root, &engine_config::GameConfig::default())
}

/// A throwaway directory under the system temp dir, removed again by `Drop`
/// so a test that asserts cannot leave litter for the next run.
pub(super) struct TempDir(PathBuf);

impl TempDir {
    pub(super) fn new(tag: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!(
            "openoxide-static-{tag}-{}-{}",
            std::process::id(),
            static_counter()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }
    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn static_counter() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}
