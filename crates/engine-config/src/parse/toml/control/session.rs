use crate::game::GameConfig;

pub(super) fn apply(cfg: &mut GameConfig, c: &toml::value::Table) {
    if let Some(s) = c.get("session").and_then(|v| v.as_table()) {
        let sec = |k: &str| {
            s.get(k)
                .and_then(|v| v.as_integer())
                .filter(|v| *v > 0)
                .map(|v| v as i64)
        };

        if let Some(v) = s
            .get("database_endpoint")
            .or_else(|| s.get("database_url"))
            .and_then(|v| v.as_str())
        {
            cfg.control.session.database_endpoint = v.to_string();
        }
        for (key, field) in [
            ("database_ns", &mut cfg.control.session.database_ns),
            ("database_db", &mut cfg.control.session.database_db),
            ("database_user", &mut cfg.control.session.database_user),
            ("database_pass", &mut cfg.control.session.database_pass),
            ("blobs", &mut cfg.control.session.blobs),
        ] {
            if let Some(v) = s.get(key).and_then(|v| v.as_str()) {
                *field = v.to_string();
            }
        }
        for (key, field) in [
            (
                "database_memtable_mib",
                &mut cfg.control.session.database_memtable_mib,
            ),
            (
                "database_cache_mib",
                &mut cfg.control.session.database_cache_mib,
            ),
        ] {
            if let Some(v) = s.get(key).and_then(|v| v.as_integer()) {
                *field = v.max(0) as u64;
            }
        }
        if let Some(v) = s.get("signing_key").and_then(|v| v.as_str()) {
            cfg.control.session.signing_key = v.to_string();
        }
        if let Some(v) = s.get("refresh_signing_key").and_then(|v| v.as_str()) {
            cfg.control.session.refresh_signing_key = v.to_string();
        }
        if let Some(v) = s.get("server_key").and_then(|v| v.as_str()) {
            cfg.control.session.server_key = v.to_string();
        }
        if let Some(v) = sec("token_expires_sec") {
            cfg.control.session.token_expiry_secs = v;
        }
        if let Some(v) = sec("refresh_token_expires_sec") {
            cfg.control.session.refresh_expiry_secs = v;
        }
    }
    if let Some(cn) = c.get("console").and_then(|v| v.as_table()) {
        if let Some(v) = cn.get("username").and_then(|v| v.as_str()) {
            cfg.control.console.username = v.to_string();
        }
        if let Some(v) = cn.get("password").and_then(|v| v.as_str()) {
            cfg.control.console.password = v.to_string();
        }
        if let Some(v) = cn.get("signing_key").and_then(|v| v.as_str()) {
            cfg.control.console.signing_key = v.to_string();
        }
        if let Some(v) = cn
            .get("token_expires_sec")
            .and_then(|v| v.as_integer())
            .filter(|v| *v > 0)
        {
            cfg.control.console.token_expiry_secs = v as i64;
        }
    }
}
