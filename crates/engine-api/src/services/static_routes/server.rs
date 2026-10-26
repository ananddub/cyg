use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::cache::Cached;
use crate::utils::static_routes::guard::child_of;

pub struct StaticServer {
    /// Project root — the only place a client-named page or asset may live.
    root: PathBuf,
    website: PathBuf,
    sdk_js: PathBuf,
    schema_mjs: PathBuf,
    /// Unity's exported build folder: `index.html` plus its own `Build/` and
    /// `TemplateData/` subtrees.
    unity_build: PathBuf,
    /// Bytes baked into the binary, used when the project has no local copy, so
    /// a fresh clone serves a working page instead of a 404.
    pub(super) defaults: HashMap<String, Arc<Vec<u8>>>,
    pub(super) cache: Mutex<HashMap<PathBuf, Cached>>,
}

impl StaticServer {
    /// `project_dir` holds `config.toml`; asset paths in the config are relative to it.
    pub fn new(project_dir: &Path, game: &engine_config::GameConfig) -> StaticServer {
        StaticServer {
            root: project_dir.to_path_buf(),
            website: project_dir.join(&game.website),
            sdk_js: game.sdk_dir.join("packages/sdk/dist/colyseus.js"),
            schema_mjs: game
                .sdk_dir
                .join("node_modules/@colyseus/schema/build/index.mjs"),
            unity_build: project_dir
                .parent()
                .map(|p| p.join("CYGArena/Build/WebGL"))
                .unwrap_or_else(|| project_dir.join("unity")),
            defaults: HashMap::new(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn default_for(&self, path: &str) -> Option<Arc<Vec<u8>>> {
        self.defaults.get(path).cloned()
    }

    pub fn website_path(&self) -> &Path {
        &self.website
    }
    pub fn sdk_js_path(&self) -> &Path {
        &self.sdk_js
    }
    pub fn schema_mjs_path(&self) -> &Path {
        &self.schema_mjs
    }
    pub fn unity_build_path(&self) -> &Path {
        &self.unity_build
    }
    pub fn project_root(&self) -> &Path {
        &self.root
    }

    /// Read `path` if it exists, refreshing the cache only when its stamp changed,
    /// which keeps the hit path to a single `stat`.
    pub fn read(&self, path: &Path) -> Option<Arc<Vec<u8>>> {
        super::cache::read(self, path)
    }

    /// Map a URL path to a file under the project root, or `None` if the client asked
    /// for something outside it.
    pub fn resolve(&self, url_path: &str) -> Option<PathBuf> {
        child_of(&self.root, url_path)
    }

    /// Map a URL path to a plain child of `base`. The percent-decode and the refusal
    /// happen in one pass, so no escape sequence can produce an unchecked path.
    pub fn resolve_under(&self, base: &Path, url_path: &str) -> Option<PathBuf> {
        child_of(base, url_path)
    }
}
