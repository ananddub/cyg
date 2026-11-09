use crate::game::GameConfig;

pub fn apply(cfg: &mut GameConfig) {
    iap(cfg);
    social(cfg);
    gameplay(cfg);
    cfg.lifecycle.apply_env();
    if let Ok(v) = std::env::var("CYG_RESULT_EVENT") {
        cfg.gameplay.result_event = v;
    }
    cfg.watchdog.apply_env();
}

fn iap(cfg: &mut GameConfig) {
    for (name, field) in [
        (
            "CYG_IAP_APPLE_SHARED_SECRET",
            &mut cfg.control.iap.apple_shared_secret,
        ),
        (
            "CYG_IAP_APPLE_VERIFY_URL",
            &mut cfg.control.iap.apple_verify_url,
        ),
        (
            "CYG_IAP_APPLE_SANDBOX_URL",
            &mut cfg.control.iap.apple_sandbox_url,
        ),
        (
            "CYG_IAP_GOOGLE_SERVICE_ACCOUNT",
            &mut cfg.control.iap.google_service_account,
        ),
        (
            "CYG_IAP_GOOGLE_PACKAGE_NAME",
            &mut cfg.control.iap.google_package_name,
        ),
        (
            "CYG_IAP_GOOGLE_TOKEN_URL",
            &mut cfg.control.iap.google_token_url,
        ),
        (
            "CYG_IAP_GOOGLE_API_BASE",
            &mut cfg.control.iap.google_api_base,
        ),
    ] {
        if let Ok(v) = std::env::var(name)
            && !v.is_empty()
        {
            *field = v;
        }
    }
    if std::env::var("CYG_IAP_ALLOW_UNVERIFIED").is_ok_and(|v| v == "1") {
        cfg.control.iap.allow_unverified = true;
    }
}

fn social(cfg: &mut GameConfig) {
    for (name, field) in [
        (
            "CYG_SOCIAL_GOOGLE_TOKENINFO_URL",
            &mut cfg.control.social.google_tokeninfo_url,
        ),
        (
            "CYG_SOCIAL_APPLE_CLIENT_ID",
            &mut cfg.control.social.apple_client_id,
        ),
        (
            "CYG_SOCIAL_APPLE_KEYS_URL",
            &mut cfg.control.social.apple_keys_url,
        ),
        (
            "CYG_SOCIAL_FACEBOOK_APP_ID",
            &mut cfg.control.social.facebook_app_id,
        ),
        (
            "CYG_SOCIAL_FACEBOOK_APP_SECRET",
            &mut cfg.control.social.facebook_app_secret,
        ),
        (
            "CYG_SOCIAL_FACEBOOK_GRAPH_URL",
            &mut cfg.control.social.facebook_graph_url,
        ),
        (
            "CYG_SOCIAL_STEAM_WEB_API_KEY",
            &mut cfg.control.social.steam_web_api_key,
        ),
        (
            "CYG_SOCIAL_STEAM_API_URL",
            &mut cfg.control.social.steam_api_url,
        ),
    ] {
        if let Ok(v) = std::env::var(name)
            && !v.is_empty()
        {
            *field = v;
        }
    }
    if let Ok(v) = std::env::var("CYG_SOCIAL_GOOGLE_CLIENT_IDS") {
        cfg.control.social.google_client_ids = crate::social::split_ids(&v);
    }
    if let Ok(v) = std::env::var("CYG_SOCIAL_STEAM_APP_ID")
        && let Ok(app_id) = v.parse()
        && app_id > 0
    {
        cfg.control.social.steam_app_id = app_id;
    }
    if std::env::var("CYG_SOCIAL_ALLOW_UNVERIFIED").is_ok_and(|v| v == "1") {
        cfg.control.social.allow_unverified = true;
    }
}

fn gameplay(cfg: &mut GameConfig) {
    if let Ok(v) = std::env::var("CYG_RECORD_EVENTS") {
        cfg.gameplay.record_events = crate::social::split_ids(&v);
    }
    if let Ok(v) = std::env::var("CYG_EVENT_TTL_DAYS") {
        cfg.gameplay.event_ttl_days = v.trim().parse().unwrap_or(cfg.gameplay.event_ttl_days);
    }
    if let Ok(v) = std::env::var("CYG_MATCH_HISTORY_TTL_DAYS") {
        cfg.gameplay.match_history_ttl_days = v
            .trim()
            .parse()
            .unwrap_or(cfg.gameplay.match_history_ttl_days);
    }
}
