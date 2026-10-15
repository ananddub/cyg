
use std::path::{Path, PathBuf};

use axum::http::{StatusCode, header};
use axum::response::Response;

pub(super) fn counter() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

pub(super) struct TempDist {
    dist: PathBuf,
}

impl TempDist {
    pub(super) fn new(tag: &str) -> TempDist {
        let root = std::env::temp_dir().join(format!(
            "openoxide-dashboard-assets-{tag}-{}-{}",
            std::process::id(),
            counter()
        ));
        let dist = root.join(crate::utils::dashboard::path_guard::DIST_SUBPATH);
        std::fs::create_dir_all(dist.join("assets")).expect("create temp dist");
        std::fs::write(dist.join("index.html"), b"<!doctype html><title>OpenOxide</title>").expect("write index");
        std::fs::write(dist.join("assets").join("app-9a7c.js"), b"console.log(1)").expect("write asset");
        TempDist { dist: std::fs::canonicalize(dist).expect("canonicalize temp dist") }
    }

    pub(super) fn dist(&self) -> &Path {
        &self.dist
    }
}

impl Drop for TempDist {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dist);
        let _ = std::fs::remove_dir_all(self.dist.parent().unwrap().parent().unwrap());
    }
}

fn header_of(response: &Response, name: header::HeaderName) -> Option<String> {
    response.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string)
}

pub(super) async fn parts(response: Response) -> (StatusCode, Option<String>, Option<String>, Vec<u8>) {
    let status = response.status();
    let content_type = header_of(&response, header::CONTENT_TYPE);
    let cache = header_of(&response, header::CACHE_CONTROL);
    let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.expect("body");
    (status, content_type, cache, body.to_vec())
}

mod serving;
mod traversal;
