use std::path::Path;
use std::sync::Arc;

use super::server::StaticServer;

#[derive(Clone)]
pub(super) struct Cached {
    pub(super) stamp: (Option<std::time::SystemTime>, u64),
    pub(super) body: Arc<Vec<u8>>,
}

pub(super) fn read(server: &StaticServer, path: &Path) -> Option<Arc<Vec<u8>>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let stamp = (meta.modified().ok(), meta.len());
    let mut cache = server.cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(hit) = cache.get(path)
        && hit.stamp == stamp
    {
        return Some(hit.body.clone());
    }
    let body = Arc::new(std::fs::read(path).ok()?);
    cache.insert(
        path.to_path_buf(),
        Cached {
            stamp,
            body: body.clone(),
        },
    );
    Some(body)
}
