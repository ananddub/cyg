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
    if let Some(body) = server
        .cache
        .read_sync(path, |_, hit| (hit.stamp == stamp).then(|| hit.body.clone()))
        .flatten()
    {
        return Some(body);
    }
    let body = Arc::new(std::fs::read(path).ok()?);
    let cached = Cached {
        stamp,
        body: body.clone(),
    };
    server
        .cache
        .entry_sync(path.to_path_buf())
        .and_modify(|hit| *hit = cached.clone())
        .or_insert(cached);
    Some(body)
}
