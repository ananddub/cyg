use engine_config::{ControlConfig, GameConfig, placeholder_secret, weak_secret};

pub(crate) fn refuse_default_secrets(
    game: &GameConfig,
    control: &ControlConfig,
) -> std::io::Result<()> {
    if std::env::var("OX_ALLOW_DEFAULT_SECRETS").as_deref() == Ok("1") {
        tracing::warn!(
            "[control] ⚠️  OX_ALLOW_DEFAULT_SECRETS=1 — booting with default secrets (DEV ONLY)"
        );
        return Ok(());
    }
    let mut defaults: Vec<&str> = Vec::new();
    let s = &control.session;
    for (name, value) in [
        ("session.signing_key", s.signing_key.as_str()),
        (
            "session.refresh_signing_key",
            s.refresh_signing_key.as_str(),
        ),
        ("session.server_key", s.server_key.as_str()),
        ("control.seat_secret", control.seat_secret.as_str()),
    ] {
        if weak_secret(value) {
            defaults.push(name);
        }
    }
    let c = &control.console;
    if c.password.trim().len() < 8 || placeholder_secret(&c.password) || weak_secret(&c.signing_key)
    {
        defaults.push("control.console.*");
    }
    let remote_db = s.database_endpoint.starts_with("ws")
        || s.database_endpoint.starts_with("http")
        || s.database_endpoint.starts_with("wss");
    if remote_db && (s.database_pass.is_empty() || weak_secret(&s.database_pass)) {
        defaults.push("session.database_pass");
    }
    if !defaults.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "refusing to start: default secrets configured ({}). Set real secrets in config.toml, or export OX_ALLOW_DEFAULT_SECRETS=1 for local dev.",
                defaults.join(", ")
            ),
        ));
    }
    Ok(())
}

pub(crate) fn describe_boot_maps(maps: &engine_config::BootMaps) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if maps.oxcl.is_some() {
        parts.push("oxcl");
    }
    if maps.map2d.is_some() {
        parts.push("map2d");
    }
    if maps.map3d.is_some() {
        parts.push("map3d");
    }
    if maps.navmesh.is_some() {
        parts.push("navmesh");
    }
    parts.join(", ")
}
