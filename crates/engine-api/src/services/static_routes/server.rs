use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::cache::Cached;
use crate::utils::static_routes::guard::child_of;

pub struct StaticServer {
    root: PathBuf,
    website: PathBuf,
    sdk_js: PathBuf,
    schema_mjs: PathBuf,
    unity_build: PathBuf,
    pub(super) defaults: HashMap<String, Arc<Vec<u8>>>,
    pub(super) cache: Mutex<HashMap<PathBuf, Cached>>,
}

impl StaticServer {
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

    pub fn read(&self, path: &Path) -> Option<Arc<Vec<u8>>> {
        super::cache::read(self, path)
    }

    pub fn resolve(&self, url_path: &str) -> Option<PathBuf> {
        child_of(&self.root, url_path)
    }

    pub fn resolve_under(&self, base: &Path, url_path: &str) -> Option<PathBuf> {
        child_of(base, url_path)
    }
}
