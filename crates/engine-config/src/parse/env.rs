use crate::game::GameConfig;

pub fn apply(cfg: &mut GameConfig) {
    session(cfg);
    console(cfg);
    if let Ok(v) = std::env::var("CYG_TICK_STAGGER")
        && let Ok(groups) = v.parse::<u32>()
    {
        cfg.tick_stagger = groups.clamp(1, 64);
    }
    if std::env::var("CYG_INPUT_COALESCE").is_ok_and(|v| v == "1" || v == "true") {
        cfg.input_coalesce = true;
    }
    crate::parse::env_social::apply(cfg);
}

fn session(cfg: &mut GameConfig) {
    if let Ok(secret) = std::env::var("CYG_SEAT_SECRET")
        && !secret.is_empty()
    {
        cfg.control.seat_secret = secret;
    }
    if std::env::var("CYG_AGONES").is_ok_and(|v| v == "1") {
        cfg.control.agones.enabled = true;
    }
    for (name, field) in [
        (
            "SURREAL_ENDPOINT",
            &mut cfg.control.session.database_endpoint,
        ),
        ("SURREAL_NS", &mut cfg.control.session.database_ns),
        ("SURREAL_DB", &mut cfg.control.session.database_db),
        ("SURREAL_USER", &mut cfg.control.session.database_user),
        ("SURREAL_PASS", &mut cfg.control.session.database_pass),
        ("CYG_BLOBS", &mut cfg.control.session.blobs),
    ] {
        if let Ok(v) = std::env::var(name)
            && !v.is_empty()
        {
            *field = v;
        }
    }
    for (name, field) in [
        (
            "CYG_DB_MEMTABLE_MIB",
            &mut cfg.control.session.database_memtable_mib,
        ),
        (
            "CYG_DB_CACHE_MIB",
            &mut cfg.control.session.database_cache_mib,
        ),
    ] {
        if let Ok(v) = std::env::var(name)
            && let Ok(mib) = v.parse()
        {
            *field = mib;
        }
    }
    if cfg.control.session.database_endpoint.is_empty() {
        for name in ["CYG_DATABASE_URL", "DATABASE_URL"] {
            if let Ok(url) = std::env::var(name)
                && !url.is_empty()
            {
                cfg.control.session.database_endpoint = url;
                break;
            }
        }
    }
    for (name, field) in [
        ("CYG_SIGNING_KEY", &mut cfg.control.session.signing_key),
        (
            "CYG_REFRESH_SIGNING_KEY",
            &mut cfg.control.session.refresh_signing_key,
        ),
        ("CYG_SERVER_KEY", &mut cfg.control.session.server_key),
    ] {
        if let Ok(v) = std::env::var(name)
            && !v.is_empty()
        {
            *field = v;
        }
    }
}

fn console(cfg: &mut GameConfig) {
    for (name, field) in [
        ("CYG_CONSOLE_USERNAME", &mut cfg.control.console.username),
        ("CYG_CONSOLE_PASSWORD", &mut cfg.control.console.password),
        (
            "CYG_CONSOLE_SIGNING_KEY",
            &mut cfg.control.console.signing_key,
        ),
    ] {
        if let Ok(v) = std::env::var(name)
            && !v.is_empty()
        {
            *field = v;
        }
    }
    if let Ok(v) = std::env::var("CYG_CONSOLE_TOKEN_EXPIRES_SEC")
        && let Ok(secs) = v.parse::<i64>()
        && secs > 0
    {
        cfg.control.console.token_expiry_secs = secs;
    }
}
