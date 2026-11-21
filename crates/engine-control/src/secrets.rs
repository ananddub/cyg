use engine_config::{ControlConfig, GameConfig, placeholder_secret, weak_secret};

/// A bind address that is reachable only from this machine.
///
/// Loopback is the signal that the operator is running locally: nothing off
/// the box can reach the console, the player API or a seat token. That is the
/// only situation where placeholder secrets are not a vulnerability.
fn loopback_only(bind_addr: &str) -> bool {
    let host = bind_addr.trim().trim_matches(|c| c == '[' || c == ']');
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

/// Where the dashboard is reachable, for the dev hint. A wildcard bind is not
/// an address a browser can open, so name the loopback it also covers.
fn console_hint(bind_addr: &str, api_port: u16) -> String {
    let host = bind_addr.trim();
    match host.parse::<std::net::IpAddr>() {
        Ok(ip) if ip.is_unspecified() => format!("127.0.0.1:{api_port}"),
        _ => format!("{host}:{api_port}"),
    }
}

/// Refuse to boot production with the shipped placeholder secrets.
///
/// Local development must not need a magic env var: `cyg run` on a project
/// that binds loopback is already a dev session, so it is allowed through with
/// a warning instead of an error the user has to look up and work around.
/// Anything reachable from another host still fails, and so does a release
/// build even on loopback unless the operator opts in explicitly — that stops
/// a copy-pasted dev config from silently shipping to a public box.
pub(crate) fn refuse_default_secrets(
    game: &GameConfig,
    control: &ControlConfig,
) -> std::io::Result<()> {
    let env_opt_in = std::env::var("CYG_ALLOW_DEFAULT_SECRETS").as_deref() == Ok("1");
    // The config opt-in has the same power as the env var, but lives in
    // config.toml so `cyg run` needs nothing exported. Both are equally
    // explicit — the operator typed one or the other.
    let explicit = env_opt_in || control.dev_mode;
    // Being bound to loopback is itself the guarantee: no other host can reach
    // the console, the player API or a seat token, so placeholder secrets leak
    // nothing. This has to hold for release binaries too — the one people
    // actually install — otherwise `cyg run` on a fresh scaffold fails with an
    // error the user has to go look up.
    let local_dev = loopback_only(&control.bind_addr);
    if explicit || local_dev {
        let why = if env_opt_in {
            "CYG_ALLOW_DEFAULT_SECRETS=1".to_string()
        } else if control.dev_mode {
            "control.dev_mode = true".to_string()
        } else {
            format!("control.bind_addr = {} is loopback-only", control.bind_addr)
        };
        tracing::warn!(
            "[control] ⚠️  {why} — booting with default secrets (DEV ONLY). \
             Set real secrets before binding this to a public address."
        );
        // Say how to get into the console while it is still local. The
        // credentials are already in config.toml; making a developer go and
        // read the file (or guess) before their first dashboard login is
        // friction with no security value, because these values are the ones
        // this very warning has just declared public to anyone on loopback.
        if local_dev {
            tracing::info!(
                "[control] 🖥️  Dashboard (DEV): http://{}/dashboard/  →  user `{}` / password `{}` \
                 (from config.toml [control.console]; change both before going public)",
                console_hint(&control.bind_addr, control.api_port),
                control.console.username,
                control.console.password
            );
        }
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
                "refusing to start: default secrets configured ({}). \
                 Set real secrets in config.toml, or set control.bind_addr = \
                 \"127.0.0.1\" for a local session (loopback is treated as dev).",
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
