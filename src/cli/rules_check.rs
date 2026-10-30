use std::path::Path;

use super::{flag_value, usage};

/// `cyg-game rules check --file <rules.json>` — validate a rule document with
/// the same evaluator the console enforces at write time; a bad body names
/// the exact field and exits nonzero.
pub fn run(args: &[String], _dir: &Path) {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        usage("rules check --file <rules.json>");
    }
    let Some(file) = flag_value(args, "--file") else {
        usage("rules check --file <rules.json>");
    };
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(e) => {
            tracing::error!("✗ rules check: cannot read {file}: {e}");
            std::process::exit(1);
        }
    };
    let body: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => {
            tracing::error!("✗ rules check: {file} is not valid JSON: {e}");
            std::process::exit(1);
        }
    };
    match engine_api::services::rules::validate(&body) {
        Ok(()) => tracing::info!("✓ {file} is a valid rule"),
        Err(err) => {
            tracing::error!("✗ rules check: {file} is invalid at {err}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn the_cli_uses_the_same_evaluator_as_the_console() {
        let good = json!({"all": [{"field": "kills", "op": "gte", "value": 10}]});
        assert!(engine_api::services::rules::validate(&good).is_ok());
        let bad = json!({"field": "kills", "op": "between", "value": 1});
        let err = engine_api::services::rules::validate(&bad).expect_err("refused");
        assert_eq!(err.path, "op");
    }
}
