use crate::game::GameConfig;

const QUOTA_KEYS: &[&str] = &[
    "enabled",
    "exempt_console",
    "player_rps",
    "player_burst",
    "default_cap",
    "default_window_secs",
];

pub(super) fn apply(cfg: &mut GameConfig, c: &toml::value::Table) {
    iap(cfg, c);
    quotas(cfg, c);
}

fn iap(cfg: &mut GameConfig, c: &toml::value::Table) {
    let Some(p) = c.get("iap").and_then(|v| v.as_table()) else {
        return;
    };
    for (key, field) in [
        (
            "apple_shared_secret",
            &mut cfg.control.iap.apple_shared_secret,
        ),
        ("apple_verify_url", &mut cfg.control.iap.apple_verify_url),
        ("apple_sandbox_url", &mut cfg.control.iap.apple_sandbox_url),
        (
            "google_service_account",
            &mut cfg.control.iap.google_service_account,
        ),
        (
            "google_package_name",
            &mut cfg.control.iap.google_package_name,
        ),
        ("google_token_url", &mut cfg.control.iap.google_token_url),
        ("google_api_base", &mut cfg.control.iap.google_api_base),
    ] {
        if let Some(v) = p.get(key).and_then(|v| v.as_str()) {
            *field = v.to_string();
        }
    }
    if let Some(v) = p.get("allow_unverified").and_then(|v| v.as_bool()) {
        cfg.control.iap.allow_unverified = v;
    }
}

fn quotas(cfg: &mut GameConfig, c: &toml::value::Table) {
    let Some(q) = c.get("quotas").and_then(|v| v.as_table()) else {
        return;
    };
    if let Some(v) = q.get("enabled").and_then(|v| v.as_bool()) {
        cfg.control.quotas.enabled = v;
    }
    if let Some(v) = q.get("exempt_console").and_then(|v| v.as_bool()) {
        cfg.control.quotas.exempt_console = v;
    }
    for (key, field) in [
        ("player_rps", &mut cfg.control.quotas.player_rps),
        ("player_burst", &mut cfg.control.quotas.player_burst),
    ] {
        if let Some(v) = q.get(key).and_then(|v| v.as_integer()).filter(|v| *v > 0) {
            *field = v as u32;
        }
    }
    if let Some(v) = q.get("default_cap").and_then(|v| v.as_integer()) {
        cfg.control.quotas.fallback.cap = v.max(0) as u32;
    }
    if let Some(v) = q
        .get("default_window_secs")
        .and_then(|v| v.as_integer())
        .filter(|v| *v > 0)
    {
        cfg.control.quotas.fallback.period_secs = v as u64;
    }

    for (name, value) in q {
        if QUOTA_KEYS.contains(&name.as_str()) {
            continue;
        }
        let mut scope = cfg.control.quotas.window(name);
        if let Some(cap) = value.as_integer() {
            scope.cap = cap.max(0) as u32;
        } else if let Some(t) = value.as_table() {
            if let Some(cap) = t.get("cap").and_then(|v| v.as_integer()) {
                scope.cap = cap.max(0) as u32;
            }
            if let Some(secs) = t
                .get("period_secs")
                .and_then(|v| v.as_integer())
                .filter(|v| *v > 0)
            {
                scope.period_secs = secs as u64;
            }
        } else {
            continue;
        }
        cfg.control.quotas.scopes.insert(name.clone(), scope);
    }
}
