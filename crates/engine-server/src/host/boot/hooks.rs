//! Loading the project's hook module.
//!
//! Hooks are account-event logic, not room logic, so they are loaded once here
//! rather than per room. A project that does not ship one runs without hooks; a
//! project whose module fails to load says which hook was expected and keeps
//! serving, because a broken hook must not stop players from joining.

use std::path::{Path, PathBuf};

use engine_config::GameConfig;
use engine_hooks::{HookEvent, HookRunner};

/// The largest document a hook may pass in either direction.
const MAX_HOOK_BYTES: usize = 64 * 1024;

pub(crate) fn boot_hooks(project_dir: &Path, cfg: &GameConfig) -> Option<HookRunner> {
    let configured = cfg.hooks_path.to_str().unwrap_or("").trim();
    let path = if configured.is_empty() {
        // Convention over configuration: `hooks/hooks.wasm` is where `cyg build`
        // puts a compiled hook module.
        let candidate = project_dir.join("hooks").join("hooks.wasm");
        if candidate.exists() {
            candidate
        } else {
            tracing::info!(
                "[server] no hooks module ([hooks] path is empty and hooks/hooks.wasm is absent) — \\
                 account events run without customer logic"
            );
            return None;
        }
    } else {
        resolve(project_dir, configured)
    };

    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!(
                "[server] ⚠️ hooks module {} could not be read ({e}) — running without hooks",
                path.display()
            );
            return None;
        }
    };

    match HookRunner::load(&bytes, MAX_HOOK_BYTES) {
        Ok(runner) => {
            let answered: Vec<&str> = runner
                .exports()
                .iter()
                .map(|event: &HookEvent| event.as_str())
                .collect();
            tracing::info!(
                "[server] 🪝 hooks module loaded ({} bytes) — answers: {}",
                bytes.len(),
                answered.join(", ")
            );
            let missing: Vec<&str> = HookEvent::ALL
                .iter()
                .filter(|event| !runner.answers(**event))
                .map(|event| event.as_str())
                .collect();
            if !missing.is_empty() {
                tracing::info!(
                    "[server] hooks not wired for: {} (these events run without customer logic)",
                    missing.join(", ")
                );
            }
            Some(runner)
        }
        Err(e) => {
            tracing::error!(
                "[server] ⚠️ hooks module {} did not load: {e} — running without hooks",
                path.display()
            );
            None
        }
    }
}

fn resolve(project_dir: &Path, configured: &str) -> PathBuf {
    let path = PathBuf::from(configured);
    if path.is_absolute() {
        path
    } else {
        project_dir.join(path)
    }
}
