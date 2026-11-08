use super::{
    ALLOW_DEFAULTS_ENV, Check, Env, MIN_PASSWORD_LEN, MIN_SECRET_LEN, placeholder_secret,
    weak_secret,
};
use crate::control::ControlConfig;
use crate::rotation::Rotation;

pub(super) fn audit(control: &ControlConfig, rotation: &Rotation, env: Env, out: &mut Vec<Check>) {
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
