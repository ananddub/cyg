mod flat;
mod toml;

use std::path::Path;

use crate::game::GameConfig;

/// Parse a TOML config, falling back to flat `key = value` / `key: value` lines.
pub fn parse_config(text: &str) -> GameConfig {
    let mut cfg = GameConfig::default();

    if let Ok(value) = text.parse::<::toml::Value>()
        && let Some(table) = value.as_table()
    {
        toml::fill(&mut cfg, table);
        cfg.lifecycle.clamp();
        return cfg;
    }

    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let (key, value) = if let Some((k, v)) = line.split_once('=') {
            (k, v)
        } else if let Some((k, v)) = line.split_once(':') {
            (k, v)
        } else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if value.is_empty() {
            continue;
        }
        flat::apply(&mut cfg, key, value);
    }
    cfg.lifecycle.clamp();
    cfg
}

/// Load `config.toml` (or `config.yml`) from a directory or file path.
pub fn load_config(path: &Path) -> std::io::Result<GameConfig> {
    let file = if path.is_dir() {
        if path.join("config.toml").exists() {
            path.join("config.toml")
        } else {
            path.join("config.yml")
        }
    } else {
        path.to_path_buf()
    };
    let text = std::fs::read_to_string(&file)?;
    let mut cfg = parse_config(&text);
    if let Ok(secret) = std::env::var("OX_SEAT_SECRET")
        && !secret.is_empty()
    {
        cfg.control.seat_secret = secret;
    }
    if std::env::var("OX_AGONES").is_ok_and(|v| v == "1") {
        cfg.control.agones.enabled = true;
    }
    // The account plane reads its target from the environment first: a
    // container sets `SURREAL_*`, and no project should have to repeat a
    // password inside a committed config.toml.
    for (name, field) in [
        ("SURREAL_ENDPOINT", &mut cfg.control.session.database_endpoint),
        ("SURREAL_NS", &mut cfg.control.session.database_ns),
        ("SURREAL_DB", &mut cfg.control.session.database_db),
        ("SURREAL_USER", &mut cfg.control.session.database_user),
        ("SURREAL_PASS", &mut cfg.control.session.database_pass),
        ("OX_BLOBS", &mut cfg.control.session.blobs),
    ] {
        if let Ok(v) = std::env::var(name)
            && !v.is_empty()
        {
            *field = v;
        }
    }
    // SurrealKV's ceilings, in MiB; `0` lets the engine derive them from the
    // host. Named `OX_DB_*` rather than `SURREAL_*`: the server's
    // `SURREAL_SURREALKV_*` variables are in bytes and the embedded SDK never
    // read them.
    for (name, field) in [
        (
            "OX_DB_MEMTABLE_MIB",
            &mut cfg.control.session.database_memtable_mib,
        ),
        ("OX_DB_CACHE_MIB", &mut cfg.control.session.database_cache_mib),
    ] {
        if let Ok(v) = std::env::var(name)
            && let Ok(mib) = v.parse()
        {
            *field = mib;
        }
    }
    // Pre-SurrealDB variable names, still honoured as endpoint aliases.
    if cfg.control.session.database_endpoint.is_empty() {
        for name in ["OX_DATABASE_URL", "DATABASE_URL"] {
            if let Ok(url) = std::env::var(name)
                && !url.is_empty()
            {
                cfg.control.session.database_endpoint = url;
                break;
            }
        }
    }
    for (name, field) in [
        ("OX_SIGNING_KEY", &mut cfg.control.session.signing_key),
        (
            "OX_REFRESH_SIGNING_KEY",
            &mut cfg.control.session.refresh_signing_key,
        ),
        ("OX_SERVER_KEY", &mut cfg.control.session.server_key),
    ] {
        if let Ok(v) = std::env::var(name)
            && !v.is_empty()
        {
            *field = v;
        }
    }
    for (name, field) in [
        ("OX_CONSOLE_USERNAME", &mut cfg.control.console.username),
        ("OX_CONSOLE_PASSWORD", &mut cfg.control.console.password),
        (
            "OX_CONSOLE_SIGNING_KEY",
            &mut cfg.control.console.signing_key,
        ),
    ] {
        if let Ok(v) = std::env::var(name)
            && !v.is_empty()
        {
            *field = v;
        }
    }
    if let Ok(v) = std::env::var("OX_CONSOLE_TOKEN_EXPIRES_SEC")
        && let Ok(secs) = v.parse::<i64>()
        && secs > 0
    {
        cfg.control.console.token_expiry_secs = secs;
    }
    for (name, field) in [
        ("OX_IAP_APPLE_SHARED_SECRET", &mut cfg.control.iap.apple_shared_secret),
        ("OX_IAP_APPLE_VERIFY_URL", &mut cfg.control.iap.apple_verify_url),
        ("OX_IAP_APPLE_SANDBOX_URL", &mut cfg.control.iap.apple_sandbox_url),
        (
            "OX_IAP_GOOGLE_SERVICE_ACCOUNT",
            &mut cfg.control.iap.google_service_account,
        ),
        (
            "OX_IAP_GOOGLE_PACKAGE_NAME",
            &mut cfg.control.iap.google_package_name,
        ),
        ("OX_IAP_GOOGLE_TOKEN_URL", &mut cfg.control.iap.google_token_url),
        ("OX_IAP_GOOGLE_API_BASE", &mut cfg.control.iap.google_api_base),
    ] {
        if let Ok(v) = std::env::var(name)
            && !v.is_empty()
        {
            *field = v;
        }
    }
    if std::env::var("OX_IAP_ALLOW_UNVERIFIED").is_ok_and(|v| v == "1") {
        cfg.control.iap.allow_unverified = true;
    }
    for (name, field) in [
        (
            "OX_SOCIAL_GOOGLE_TOKENINFO_URL",
            &mut cfg.control.social.google_tokeninfo_url,
        ),
        ("OX_SOCIAL_APPLE_CLIENT_ID", &mut cfg.control.social.apple_client_id),
        ("OX_SOCIAL_APPLE_KEYS_URL", &mut cfg.control.social.apple_keys_url),
        (
            "OX_SOCIAL_FACEBOOK_APP_ID",
            &mut cfg.control.social.facebook_app_id,
        ),
        (
            "OX_SOCIAL_FACEBOOK_APP_SECRET",
            &mut cfg.control.social.facebook_app_secret,
        ),
        (
            "OX_SOCIAL_FACEBOOK_GRAPH_URL",
            &mut cfg.control.social.facebook_graph_url,
        ),
        (
            "OX_SOCIAL_STEAM_WEB_API_KEY",
            &mut cfg.control.social.steam_web_api_key,
        ),
        ("OX_SOCIAL_STEAM_API_URL", &mut cfg.control.social.steam_api_url),
    ] {
        if let Ok(v) = std::env::var(name)
            && !v.is_empty()
        {
            *field = v;
        }
    }
    if let Ok(v) = std::env::var("OX_SOCIAL_GOOGLE_CLIENT_IDS") {
        cfg.control.social.google_client_ids = crate::social::split_ids(&v);
    }
    if let Ok(v) = std::env::var("OX_SOCIAL_STEAM_APP_ID")
        && let Ok(app_id) = v.parse()
        && app_id > 0
    {
        cfg.control.social.steam_app_id = app_id;
    }
    if std::env::var("OX_SOCIAL_ALLOW_UNVERIFIED").is_ok_and(|v| v == "1") {
        cfg.control.social.allow_unverified = true;
    }
    for (name, field) in [
        ("OX_ROOM_MAX_SECONDS", &mut cfg.lifecycle.room_max_seconds),
        ("OX_REAP_IDLE_SECS", &mut cfg.lifecycle.reap_idle_secs),
        ("OX_PEER_IDLE_MS", &mut cfg.lifecycle.peer_idle_ms),
        ("OX_SEAT_HOLD_SECS", &mut cfg.lifecycle.seat_hold_secs),
    ] {
        if let Ok(v) = std::env::var(name)
            && let Ok(secs) = v.parse::<u64>()
        {
            *field = secs;
        }
    }
    if let Ok(v) = std::env::var("OX_PERSISTENT_ROOMS") {
        cfg.lifecycle.persistent_rooms = crate::social::split_ids(&v);
    }
    cfg.lifecycle.clamp();
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lifecycle_block_replaces_the_host_windows() {
        let cfg = parse_config(
            r#"
[lifecycle]
room_max_seconds = 900
reap_idle_secs = 5
peer_idle_ms = 800
seat_hold_secs = 45
persistent_rooms = ["lobby", "hub"]
"#,
        );
        assert_eq!(cfg.lifecycle.room_max_seconds, 900);
        assert_eq!(cfg.lifecycle.reap_idle_secs, 5);
        assert_eq!(cfg.lifecycle.peer_idle_ms, 800);
        assert_eq!(cfg.lifecycle.seat_hold_secs, 45);
        assert!(cfg.lifecycle.is_persistent("hub"));
        assert!(!cfg.lifecycle.is_persistent("arena"));
    }

    /// A flat `key: value` project file reads the same windows, and an
    /// out-of-range one is pulled back instead of handed to the loop (a zero
    /// reap would mean "dispose the instant a room empties").
    #[test]
    fn flat_lifecycle_keys_are_read_and_clamped() {
        let cfg = parse_config(
            "room_max_seconds: 0\nreap_idle_secs: 0\nseat_hold_secs: 99999999999\n\
             persistent_rooms: lobby, hub\n",
        );
        assert_eq!(cfg.lifecycle.room_max_seconds, 0);
        assert_eq!(cfg.lifecycle.reap_idle_secs, 1);
        assert_eq!(cfg.lifecycle.seat_hold_secs, 7 * 24 * 60 * 60);
        assert_eq!(cfg.lifecycle.persistent_rooms, ["lobby", "hub"]);
    }
}
