use std::path::Path;

use super::load_session;

/// `cyg-game secrets audit [dir]` — run the boot guard's secret checks and
/// exit nonzero on the first class of problem a deploy would refuse.
pub fn run(dir: &Path) {
    let cfg = match engine_config::load_config(dir) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("✗ secrets audit: {e}");
            std::process::exit(1);
        }
    };
    let problems = audit(cfg.control);
    if problems > 0 {
        tracing::info!("  rotation runbook: docs/SECRETS.md");
        std::process::exit(1);
    }
    tracing::info!("✓ every secret passes the audit");
}

/// Shared with `doctor`; returns the number of failed checks and prints each.
pub fn audit(control: engine_config::ControlConfig) -> usize {
    let read_env = |name: &str| std::env::var(name).ok();
    let checks =
        engine_config::audit_secrets(&control, engine_config::Rotation::global(), &read_env);
    let mut problems = 0usize;
    for check in &checks {
        let mark = if check.ok { "✓" } else { "✗" };
        tracing::info!("{mark} {}", check.detail);
        problems += usize::from(!check.ok);
    }
    problems
}
