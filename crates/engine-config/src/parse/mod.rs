mod flat;
mod toml;

use std::path::Path;

use crate::game::GameConfig;

/// Pull every operator-written window into the range the host can act on, and
/// tidy every operator-written list the host reads per message.
fn clamp_windows(cfg: &mut GameConfig) {
    cfg.lifecycle.clamp();
    cfg.watchdog.clamp();
    cfg.gameplay.normalize();
}

/// Parse a TOML config, falling back to flat `key = value` / `key: value` lines.
pub fn parse_config(text: &str) -> GameConfig {
    let mut cfg = GameConfig::default();

    if let Ok(value) = text.parse::<::toml::Value>()
        && let Some(table) = value.as_table()
    {
        toml::fill(&mut cfg, table);
        clamp_windows(&mut cfg);
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
    clamp_windows(&mut cfg);
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
        (
            "SURREAL_ENDPOINT",
            &mut cfg.control.session.database_endpoint,
        ),
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
        (
            "OX_DB_CACHE_MIB",
            &mut cfg.control.session.database_cache_mib,
        ),
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
        (
            "OX_IAP_APPLE_SHARED_SECRET",
            &mut cfg.control.iap.apple_shared_secret,
        ),
        (
            "OX_IAP_APPLE_VERIFY_URL",
            &mut cfg.control.iap.apple_verify_url,
        ),
        (
            "OX_IAP_APPLE_SANDBOX_URL",
            &mut cfg.control.iap.apple_sandbox_url,
        ),
        (
            "OX_IAP_GOOGLE_SERVICE_ACCOUNT",
            &mut cfg.control.iap.google_service_account,
        ),
        (
            "OX_IAP_GOOGLE_PACKAGE_NAME",
            &mut cfg.control.iap.google_package_name,
        ),
        (
            "OX_IAP_GOOGLE_TOKEN_URL",
            &mut cfg.control.iap.google_token_url,
        ),
        (
            "OX_IAP_GOOGLE_API_BASE",
            &mut cfg.control.iap.google_api_base,
        ),
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
        (
            "OX_SOCIAL_APPLE_CLIENT_ID",
            &mut cfg.control.social.apple_client_id,
        ),
        (
            "OX_SOCIAL_APPLE_KEYS_URL",
            &mut cfg.control.social.apple_keys_url,
        ),
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
        (
            "OX_SOCIAL_STEAM_API_URL",
            &mut cfg.control.social.steam_api_url,
        ),
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
    if let Ok(v) = std::env::var("OX_RECORD_EVENTS") {
        cfg.gameplay.record_events = crate::social::split_ids(&v);
    }
    if let Ok(v) = std::env::var("OX_EVENT_TTL_DAYS") {
        cfg.gameplay.event_ttl_days = v.trim().parse().unwrap_or(cfg.gameplay.event_ttl_days);
    }
    if let Ok(v) = std::env::var("OX_MATCH_HISTORY_TTL_DAYS") {
        cfg.gameplay.match_history_ttl_days = v
            .trim()
            .parse()
            .unwrap_or(cfg.gameplay.match_history_ttl_days);
    }
    cfg.lifecycle.apply_env();
    if let Ok(v) = std::env::var("OX_RESULT_EVENT") {
        cfg.gameplay.result_event = v;
    }
    cfg.watchdog.apply_env();
    clamp_windows(&mut cfg);
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

    /// The watchdog rides the same two readers as `[lifecycle]`, so a shipped
    /// game cannot be the one project whose guest timeout silently stayed off.
    #[test]
    fn a_watchdog_block_replaces_the_fuse() {
        let cfg = parse_config(
            r#"
[watchdog]
enabled = false
tick_budget_ms = 2000
epoch_deadline_ticks = 8
"#,
        );
        assert!(!cfg.watchdog.enabled);
        assert_eq!(cfg.watchdog.tick_budget_ms, 2000);
        assert_eq!(cfg.watchdog.epoch_deadline_ticks, 8);
    }

    #[test]
    fn flat_watchdog_keys_are_read_and_clamped() {
        let cfg = parse_config("tick_budget_ms: 5\nepoch_deadline_ticks: 0\n");
        assert!(
            cfg.watchdog.enabled,
            "only an explicit switch can disable it"
        );
        assert_eq!(cfg.watchdog.tick_budget_ms, 250);
        assert_eq!(cfg.watchdog.epoch_deadline_ticks, 1);
    }

    /// Nothing is recorded unless the game asks: the default is an empty list,
    /// so a guest's chat traffic cannot reach the database on its own.
    #[test]
    fn the_event_allowlist_is_empty_until_a_game_lists_it() {
        assert!(parse_config("").gameplay.record_events.is_empty());
        let cfg = parse_config(
            r#"
[gameplay]
record_events = ["discard", "deal"]
"#,
        );
        assert!(cfg.gameplay.records("discard"));
        assert!(!cfg.gameplay.records("chat"));
    }

    #[test]
    fn flat_record_events_are_trimmed_and_de_duplicated() {
        let cfg = parse_config("record_events: discard,  discard ,\n");
        assert_eq!(cfg.gameplay.record_events, ["discard"]);
    }

    #[test]
    fn both_retention_windows_are_read_from_the_same_block() {
        let cfg = parse_config(
            r#"
[gameplay]
event_ttl_days = 14
match_history_ttl_days = 365
"#,
        );
        assert_eq!(cfg.gameplay.event_ttl_days, 14);
        assert_eq!(cfg.gameplay.match_history_ttl_days, 365);
    }

    /// A bare number is a scope's cap and keeps that scope's window; a table sets
    /// both. Anything the game did not name falls back to the default window.
    #[test]
    fn a_quota_block_tunes_the_windows_it_names() {
        use crate::quota::{self, QuotaConfig};
        let cfg = parse_config(
            r#"
[control.quotas]
enabled = true
player_rps = 5
player_burst = 7
login = 3
[control.quotas."channel.send"]
cap = 10
period_secs = 30
"#,
        );
        let quotas = &cfg.control.quotas;
        assert!(quotas.enabled);
        assert!(
            quotas.exempt_console,
            "exemption holds until an operator drops it"
        );
        assert_eq!(quotas.player_rps, 5);
        assert_eq!(quotas.player_burst, 7);
        assert_eq!(quotas.window(quota::LOGIN).cap, 3);
        assert_eq!(
            quotas.window(quota::LOGIN).period_secs,
            QuotaConfig::default().window(quota::LOGIN).period_secs
        );
        assert_eq!(quotas.window(quota::CHANNEL_SEND).cap, 10);
        assert_eq!(quotas.window(quota::CHANNEL_SEND).period_secs, 30);
        assert_eq!(quotas.window("nobody.listed.this").cap, 60, "the fallback");
    }

    #[test]
    fn a_quota_scope_can_be_switched_off_with_a_zero_cap() {
        use crate::quota;
        let cfg = parse_config("[control.quotas]\nenabled = true\nlogin = 0\n");
        assert_eq!(cfg.control.quotas.window(quota::LOGIN).cap, 0);
    }
}
