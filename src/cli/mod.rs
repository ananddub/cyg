pub mod config_diff;
pub mod logs_tail;
pub mod migrate;
pub mod openapi;
pub mod rules_check;
pub mod secrets;

use std::path::Path;

pub(crate) fn block_on<T>(
    future: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(future)
}

pub(crate) fn usage(line: &str) -> ! {
    tracing::info!("Usage: cyg-game {line}");
    std::process::exit(1)
}

/// The positional (non-flag, non-flag-value) arguments of a subcommand.
pub(crate) fn positional(args: &[String]) -> Vec<&str> {
    let mut skip = false;
    args.iter()
        .filter(|a| {
            if skip {
                skip = false;
                return false;
            }
            if a.starts_with("--") {
                skip = true;
                return false;
            }
            true
        })
        .map(String::as_str)
        .collect()
}

pub(crate) fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

pub(crate) fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

pub(crate) fn load_session(dir: &Path) -> engine_config::SessionConfig {
    match engine_config::load_config(dir) {
        Ok(cfg) => cfg.control.session,
        Err(e) => {
            tracing::error!("✗ {e}");
            std::process::exit(1);
        }
    }
}

pub(crate) async fn open_datastore(
    session: &engine_config::SessionConfig,
) -> Result<engine_api::db::Datastore, String> {
    let user = (!session.database_user.is_empty()).then_some(session.database_user.as_str());
    let pass = (!session.database_pass.is_empty()).then_some(session.database_pass.as_str());
    engine_api::db::Datastore::open(
        &session.database_endpoint,
        &session.database_ns,
        &session.database_db,
        user,
        pass,
        engine_api::db::KvBudget::default(),
    )
    .await
    .map_err(|e| format!("{}: {e}", session.database_endpoint))
}
