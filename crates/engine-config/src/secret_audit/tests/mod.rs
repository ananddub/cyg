mod basics;
mod edge_cases;

use super::*;
use crate::control::ControlConfig;
use crate::rotation::Rotation;

fn strong() -> ControlConfig {
    let mut c = ControlConfig::default();
    c.session.signing_key = "a-signing-key-long-enough".into();
    c.session.refresh_signing_key = "a-refresh-key-long-enough".into();
    c.session.server_key = "a-server-key-long-enough".into();
    c.seat_secret = "a-seat-secret-long-enough".into();
    c.console.signing_key = "a-console-key-long-enough".into();
    c.console.password = "a-real-password".into();
    c
}

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let owned: Vec<(String, String)> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |name| {
        owned
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    }
}

fn audit_strong(pairs: &[(&str, &str)]) -> Vec<Check> {
    audit_secrets(&strong(), &Rotation::default(), &env(pairs))
}

fn problems(checks: &[Check]) -> usize {
    checks.iter().filter(|c| !c.ok).count()
}

fn failures(checks: &[Check]) -> Vec<&str> {
    checks
        .iter()
        .filter(|c| !c.ok)
        .map(|c| c.detail.as_str())
        .collect()
}
