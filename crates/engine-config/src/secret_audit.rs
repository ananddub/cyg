//! The rules a secret has to satisfy, and the audit `doctor` runs with them.
//!
//! `engine-control` refuses to boot a plane on a documented default. This module
//! asks the same questions of a config file and an environment, so one command
//! lists every problem before a deploy instead of the first one that happens to
//! stop a process from starting.

use crate::control::ControlConfig;
use crate::rotation::Rotation;

/// Shortest key worth trusting with an HMAC. A signing key below this is
/// guessable offline, which is a full-console compromise rather than a nuisance.
pub const MIN_SECRET_LEN: usize = 16;

/// A console password is a login policy, not a key, so it gets its own floor.
pub const MIN_PASSWORD_LEN: usize = 8;

/// The escape hatch that lets a developer boot on defaults.
pub const ALLOW_DEFAULTS_ENV: &str = "OX_ALLOW_DEFAULT_SECRETS";

/// A documented default or template placeholder a stranger could guess.
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

/// Enough entropy to survive an offline guess, and not a known default.
pub fn weak_secret(value: &str) -> bool {
    placeholder_secret(value) || value.trim().len() < MIN_SECRET_LEN
}

/// One line of the audit. `ok == false` is a reason not to ship.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub ok: bool,
    pub detail: String,
}

impl Check {
    fn pass(detail: String) -> Check {
        Check { ok: true, detail }
    }

    fn fail(detail: String) -> Check {
        Check { ok: false, detail }
    }
}

/// Whether a flag spells "off". An unset flag is off; a value that is neither a
/// recognised on nor a recognised off is an error, because the config parser
/// silently treats it as unset and nobody can tell after the fact what the
/// operator meant.
pub fn flag_is_off(raw: Option<&str>) -> Result<bool, String> {
    let Some(raw) = raw else { return Ok(true) };
    match raw.trim().to_ascii_lowercase().as_str() {
        "" => Err("set but empty".into()),
        "0" | "false" | "off" | "no" => Ok(true),
        "1" | "true" | "on" | "yes" => Ok(false),
        other => Err(other.into()),
    }
}

type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Everything that can be said about a deployment's secrets without starting a
/// process. `env` is injected rather than read from the environment because
/// mutating process env in a test is `unsafe` in edition 2024.
pub fn audit_secrets(control: &ControlConfig, rotation: &Rotation, env: Env) -> Vec<Check> {
    let mut checks = Vec::new();
    audit_strength(control, rotation, env, &mut checks);
    audit_unverified(control, env, &mut checks);
    checks
}

fn audit_strength(control: &ControlConfig, rotation: &Rotation, env: Env, out: &mut Vec<Check>) {
    if env(ALLOW_DEFAULTS_ENV).as_deref() == Some("1") {
        out.push(Check::pass(
            "the boot guard is switched off (OX_ALLOW_DEFAULT_SECRETS=1), so secret strength is not audited. Local dev only: the manifests in deploy/k8s must never set it".into(),
        ));
    } else {
        let s = &control.session;
        for (name, value) in [
            ("session.signing_key", s.signing_key.as_str()),
            (
                "session.refresh_signing_key",
                s.refresh_signing_key.as_str(),
            ),
            ("session.server_key", s.server_key.as_str()),
            ("control.seat_secret", control.seat_secret.as_str()),
            (
                "control.console.signing_key",
                control.console.signing_key.as_str(),
            ),
        ] {
            out.push(if weak_secret(value) {
                Check::fail(format!(
                    "{name} is empty, a documented default, or shorter than {MIN_SECRET_LEN} characters"
                ))
            } else {
                Check::pass(format!("{name} is set ({} characters)", value.trim().len()))
            });
        }

        let password = control.console.password.as_str();
        out.push(if placeholder_secret(password) || password.trim().len() < MIN_PASSWORD_LEN {
            Check::fail(format!(
                "control.console.password is empty, a documented default, or shorter than {MIN_PASSWORD_LEN} characters"
            ))
        } else {
            Check::pass("control.console.password is set".into())
        });

        let remote =
            s.database_endpoint.starts_with("ws") || s.database_endpoint.starts_with("http");
        out.push(if !remote {
            Check::pass(format!(
                "the database is embedded ({}), so there is no network credential to guard",
                s.database_endpoint
            ))
        } else if weak_secret(&s.database_pass) {
            Check::fail(format!(
                "session.database_pass is a default or shorter than {MIN_SECRET_LEN} characters while {} is reachable over the network",
                s.database_endpoint
            ))
        } else {
            Check::pass("session.database_pass is set for a remote database".into())
        });
    }

    // The dual-key window only exists while the retired secret is still trusted,
    // and everything minted with it keeps verifying today — so it has to be as
    // unguessable as the key that replaced it.
    if !rotation.rotating() {
        out.push(Check::pass("no retired secret is being accepted".into()));
    }
    for (name, value) in rotation.previous_secrets() {
        out.push(if weak_secret(value) {
            Check::fail(format!(
                "{name} is still accepted for verification but is a default or shorter than {MIN_SECRET_LEN} characters"
            ))
        } else {
            Check::pass(format!("{name} is accepted for one rotation window"))
        });
    }
}

fn audit_unverified(control: &ControlConfig, env: Env, out: &mut Vec<Check>) {
    for (name, configured) in [
        ("OX_IAP_ALLOW_UNVERIFIED", control.iap.allow_unverified),
        (
            "OX_SOCIAL_ALLOW_UNVERIFIED",
            control.social.allow_unverified,
        ),
    ] {
        match flag_is_off(env(name).as_deref()) {
            Err(value) => out.push(Check::fail(format!(
                "{name}={value:?} is neither on nor off; the parser reads it as unset, so what was meant is unknowable — write 0 or delete the line"
            ))),
            Ok(off) => out.push(if off && !configured {
                Check::pass(format!("{name} is off"))
            } else {
                Check::fail(format!(
                    "{} is on: {}",
                    name,
                    unverified_reason(name)
                ))
            }),
        }
    }
}

/// Why an unverified-acceptance flag can never be on in a deployment.
pub fn unverified_reason(name: &str) -> String {
    match name {
        "OX_IAP_ALLOW_UNVERIFIED" => "store receipts are trusted without asking the store, so any client can mint currency".into(),
        "OX_SOCIAL_ALLOW_UNVERIFIED" => "social tokens are mapped to an identity without asking the issuer, so any client can pick an account to log in as".into(),
        other => format!("{other} accepts a credential nobody verified"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_configured_deployment_passes_the_audit() {
        let checks = audit_strong(&[]);
        assert_eq!(problems(&checks), 0, "{checks:?}");
    }

    #[test]
    fn every_documented_default_is_named_in_the_report() {
        let checks = audit_secrets(&ControlConfig::default(), &Rotation::default(), &env(&[]));
        let failed = failures(&checks);
        for name in [
            "session.signing_key",
            "session.refresh_signing_key",
            "session.server_key",
            "control.seat_secret",
            "control.console.signing_key",
            "control.console.password",
        ] {
            assert!(
                failed.iter().any(|d| d.starts_with(name)),
                "{name} must be reported: {failed:?}"
            );
        }
    }

    #[test]
    fn doctor_fails_when_a_credential_is_accepted_unverified() {
        for (name, why) in [
            ("OX_IAP_ALLOW_UNVERIFIED", "currency"),
            ("OX_SOCIAL_ALLOW_UNVERIFIED", "identity"),
        ] {
            for value in ["1", "true"] {
                let checks = audit_strong(&[(name, value)]);
                assert_eq!(problems(&checks), 1, "{name}={value}");
                assert!(
                    failures(&checks)[0].contains(name) && failures(&checks)[0].contains(why),
                    "{name}={value} must say why: {:?}",
                    failures(&checks)
                );
            }
        }
        let mut configured = strong();
        configured.iap.allow_unverified = true;
        configured.social.allow_unverified = true;
        assert_eq!(
            problems(&audit_secrets(&configured, &Rotation::default(), &env(&[]))),
            2,
            "the config spelling fails exactly like the env"
        );
    }

    #[test]
    fn a_flag_value_that_is_neither_on_nor_off_fails_instead_of_being_ignored() {
        for value in ["maybe", "", "01"] {
            let checks = audit_strong(&[("OX_IAP_ALLOW_UNVERIFIED", value)]);
            assert_eq!(problems(&checks), 1, "{value:?}");
            assert!(
                failures(&checks)[0].contains("neither on nor off"),
                "{value:?}: {:?}",
                failures(&checks)
            );
        }
        for value in ["0", "false", "off", "no"] {
            assert_eq!(
                problems(&audit_strong(&[("OX_IAP_ALLOW_UNVERIFIED", value)])),
                0,
                "{value:?} is an explicit off"
            );
        }
    }

    #[test]
    fn the_development_escape_hatch_skips_strength_but_not_flags() {
        let checks = audit_secrets(
            &ControlConfig::default(),
            &Rotation::default(),
            &env(&[
                ("OX_ALLOW_DEFAULT_SECRETS", "1"),
                ("OX_IAP_ALLOW_UNVERIFIED", "1"),
            ]),
        );
        assert_eq!(problems(&checks), 1, "{checks:?}");
        assert!(
            checks
                .iter()
                .all(|c| !c.detail.contains("session.signing_key")),
            "the guard is off, so strength is not this audit's job"
        );
    }

    #[test]
    fn a_retired_secret_has_to_be_as_unguessable_as_the_key_it_replaced() {
        let rotation = Rotation {
            previous_signing_key: "old".into(),
            previous_seat_secret: "a-retired-seat-secret-long-enough".into(),
            ..Default::default()
        };
        let checks = audit_secrets(&strong(), &rotation, &env(&[]));
        assert_eq!(problems(&checks), 1, "{checks:?}");
        assert!(failures(&checks)[0].contains("OX_PREVIOUS_SIGNING_KEY"));
        assert!(
            checks
                .iter()
                .any(|c| c.ok && c.detail.contains("OX_PREVIOUS_SEAT_SECRET"))
        );
    }

    #[test]
    fn a_remote_database_needs_a_password_worth_the_transport() {
        let mut c = strong();
        c.session.database_endpoint = "ws://db:8000/rpc".into();
        assert_eq!(
            problems(&audit_secrets(&c, &Rotation::default(), &env(&[]))),
            1
        );
        c.session.database_pass = "s3cret".into();
        assert_eq!(
            problems(&audit_secrets(&c, &Rotation::default(), &env(&[]))),
            1
        );
        c.session.database_pass = "a-long-enough-database-pass".into();
        assert_eq!(
            problems(&audit_secrets(&c, &Rotation::default(), &env(&[]))),
            0
        );
    }
}
