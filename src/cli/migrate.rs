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

/// `cyg-game schema-status [dir]` — report the applied schema versions and the
/// pending migration steps without touching anything.
pub fn status(dir: &Path) {
    let session = load_session(dir);
    if session.database_endpoint.is_empty() {
        tracing::error!("✗ schema-status: no database endpoint is configured");
        std::process::exit(1);
    }
    match super::block_on(async {
        let db = super::open_datastore(&session).await?;
        let applied = engine_api::db::migrations::applied(&db)
            .await
            .map_err(|e| {
                format!("schema_version unreadable (boot the server once to create it): {e}")
            })?;
        let pending =
            engine_api::db::migrations::pending(&db, engine_api::db::migrations::MIGRATIONS)
                .await
                .map_err(|e| e.to_string())?;
        Ok::<_, String>((applied.into_iter().map(|a| (a.version, a.name)), pending))
    }) {
        Ok((applied, pending)) => {
            let versions: Vec<String> = applied
                .map(|(version, name)| format!("{version} {name}"))
                .collect();
            if versions.is_empty() {
                tracing::info!("• schema at base (no migration steps applied yet)");
            } else {
                for row in &versions {
                    tracing::info!("✓ applied {row}");
                }
            }
            if pending.is_empty() {
                tracing::info!("✓ no pending migration steps");
            } else {
                for name in &pending {
                    tracing::info!("• pending: {name}");
                }
            }
        }
        Err(e) => {
            tracing::error!("✗ schema-status: {e}");
            std::process::exit(1);
        }
    }
}
