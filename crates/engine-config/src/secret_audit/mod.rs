mod strength;
#[cfg(test)]
mod tests;
mod unverified;

use crate::control::ControlConfig;
use crate::rotation::Rotation;

pub const MIN_SECRET_LEN: usize = 16;

pub const MIN_PASSWORD_LEN: usize = 8;

pub const ALLOW_DEFAULTS_ENV: &str = "OX_ALLOW_DEFAULT_SECRETS";

pub fn placeholder_secret(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.is_empty()
        || lower.starts_with("default")
        || lower.starts_with("ox-dev")
        || lower.contains("change-me")
        || lower.contains("changeme")
        || lower.contains("placeholder")
        || matches!(lower.as_str(), "password" | "secret" | "test" | "letmein")
}

pub fn weak_secret(value: &str) -> bool {
    placeholder_secret(value) || value.trim().len() < MIN_SECRET_LEN
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub ok: bool,
    pub detail: String,
}

impl Check {
    pub(super) fn pass(detail: String) -> Check {
        Check { ok: true, detail }
    }

    pub(super) fn fail(detail: String) -> Check {
        Check { ok: false, detail }
    }
}

pub fn flag_is_off(raw: Option<&str>) -> Result<bool, String> {
    let Some(raw) = raw else { return Ok(true) };
    match raw.trim().to_ascii_lowercase().as_str() {
        "" => Err("set but empty".into()),
        "0" | "false" | "off" | "no" => Ok(true),
        "1" | "true" | "on" | "yes" => Ok(false),
        other => Err(other.into()),
    }
}

pub(super) type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

pub fn audit_secrets(control: &ControlConfig, rotation: &Rotation, env: Env) -> Vec<Check> {
    let mut checks = Vec::new();
    strength::audit(control, rotation, env, &mut checks);
    unverified::audit(control, env, &mut checks);
    checks
}

pub fn unverified_reason(name: &str) -> String {
    match name {
        "OX_IAP_ALLOW_UNVERIFIED" => "store receipts are trusted without asking the store, so any client can mint currency".into(),
        "OX_SOCIAL_ALLOW_UNVERIFIED" => "social tokens are mapped to an identity without asking the issuer, so any client can pick an account to log in as".into(),
        other => format!("{other} accepts a credential nobody verified"),
    }
}
