use crate::game::GameConfig;

pub(super) fn apply(cfg: &mut GameConfig, c: &toml::value::Table) {
    let Some(s) = c.get("social").and_then(|v| v.as_table()) else {
        return;
    };
    if let Some(v) = s.get("google_client_ids") {
        if let Some(ids) = v.as_array() {
            cfg.control.social.google_client_ids = ids
                .iter()
                .filter_map(|id| id.as_str())
                .map(str::to_owned)
                .collect();
        } else if let Some(ids) = v.as_str() {
            cfg.control.social.google_client_ids = crate::social::split_ids(ids);
        }
    }
    for (key, field) in [
        (
            "google_tokeninfo_url",
            &mut cfg.control.social.google_tokeninfo_url,
        ),
        ("apple_client_id", &mut cfg.control.social.apple_client_id),
        ("apple_keys_url", &mut cfg.control.social.apple_keys_url),
        ("facebook_app_id", &mut cfg.control.social.facebook_app_id),
        (
            "facebook_app_secret",
            &mut cfg.control.social.facebook_app_secret,
        ),
        (
            "facebook_graph_url",
            &mut cfg.control.social.facebook_graph_url,
        ),
        (
            "steam_web_api_key",
            &mut cfg.control.social.steam_web_api_key,
        ),
        ("steam_api_url", &mut cfg.control.social.steam_api_url),
    ] {
        if let Some(v) = s.get(key).and_then(|v| v.as_str()) {
            *field = v.to_string();
        }
    }
    if let Some(v) = s
        .get("steam_app_id")
        .and_then(|v| v.as_integer())
        .filter(|v| *v > 0)
    {
        cfg.control.social.steam_app_id = v as u32;
    }
    if let Some(v) = s.get("allow_unverified").and_then(|v| v.as_bool()) {
        cfg.control.social.allow_unverified = v;
    }
}
