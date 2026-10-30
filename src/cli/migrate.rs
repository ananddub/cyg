use std::path::Path;

use super::{block_on, load_session};

/// `cyg-game migrate [dir]` — apply the pending schema steps to the configured
/// database and print what ran. The server applies them at boot too; this arm
/// exists for deployments that want the step to happen explicitly, before the
/// process starts.
pub fn run(dir: &Path) {
    let session = load_session(dir);
    if session.database_endpoint.is_empty() {
        tracing::error!("✗ migrate: no database endpoint is configured");
        std::process::exit(1);
    }
    match block_on(async {
        let db = super::open_datastore(&session).await?;
        engine_api::db::migrations::apply_migrations(&db, engine_api::db::migrations::MIGRATIONS)
            .await
            .map_err(|e| e.to_string())
    }) {
        Ok(steps) if steps.is_empty() => {
            tracing::info!("✓ schema is up to date; nothing to migrate");
        }
        Ok(steps) => {
            for name in &steps {
                tracing::info!("✓ applied {name}");
            }
            tracing::info!("✓ {} migration step(s) applied", steps.len());
        }
        Err(e) => {
            tracing::error!("✗ migrate: {e}");
            std::process::exit(1);
        }
    }
}
