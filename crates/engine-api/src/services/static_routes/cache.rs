use std::path::Path;
use std::sync::Arc;

use super::server::StaticServer;

/// A file plus the stamp it was read at: a re-read happens only when the stat disagrees, so
/// a hot page is one `stat` per request. The length catches a same-second rewrite.
#[derive(Clone)]
pub(super) struct Cached {
    pub(super) stamp: (Option<std::time::SystemTime>, u64),
    pub(super) body: Arc<Vec<u8>>,
}

/// `None` on any stat or read failure — a missing asset is a 404, never a
/// panic and never an empty body cached forever.
pub(super) fn read(server: &StaticServer, path: &Path) -> Option<Arc<Vec<u8>>> {
    // The only syscall on the hit path.
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let stamp = (meta.modified().ok(), meta.len());
    // Poison-tolerant: a panic somewhere else must not turn every later
    // asset request into a 404 for the life of the process.
    let mut cache = server.cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(hit) = cache.get(path)
        && hit.stamp == stamp
    {
        return Some(hit.body.clone());
    }
    // Re-read happens on a changed stamp only; on failure nothing is
    // inserted, so the next request stats again instead of serving stale.
    let body = Arc::new(std::fs::read(path).ok()?);
    cache.insert(path.to_path_buf(), Cached { stamp, body: body.clone() });
    Some(body)
}
