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
    Ok(cfg)
}
