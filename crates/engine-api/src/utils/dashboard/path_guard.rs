use std::path::{Path, PathBuf};

pub const ENV_DIST: &str = "OX_DASHBOARD_DIST";

pub const DIST_SUBPATH: &str = "dashboard/dist";

pub const ASSET_PREFIX: &str = "/dashboard/assets/";

pub fn dist_root() -> Option<PathBuf> {
    if let Some(from_env) = std::env::var_os(ENV_DIST)
        && let Some(dir) = usable_dist(Path::new(&from_env))
    {
        return Some(dir);
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if let Some(found) = usable_dist(&dir.join(DIST_SUBPATH)) {
            return Some(found);
        }
        if !dir.pop() {
            return None;
        }
    }
}

pub fn usable_dist(dir: &Path) -> Option<PathBuf> {
    if !dir.join("index.html").is_file() {
        return None;
    }
    std::fs::canonicalize(dir).ok()
}

/// Path-traversal guard: rejects `..`, `\`, NUL and `%` segments outright,
/// then canonicalises so a symlinked file cannot escape `dist`.
pub fn child_under(dist: &Path, rest: &str) -> Option<PathBuf> {
    if rest.is_empty() || rest.starts_with('/') {
        return None;
    }
    let mut out = dist.to_path_buf();
    for seg in rest.split('/') {
        match seg {
            "" | "." => continue,
            ".." => return None,
            _ => {
                if seg.contains(['\\', '\0', '%']) {
                    return None;
                }
                out.push(seg);
            }
        }
    }
    let found = std::fs::canonicalize(out).ok()?;
    found
        .is_file()
        .then_some(found)
        .filter(|p| p.starts_with(dist))
}
