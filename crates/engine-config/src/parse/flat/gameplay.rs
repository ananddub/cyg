use crate::game::GameConfig;

pub(super) fn apply(cfg: &mut GameConfig, key: &str, value: &str) {
    match key {
        "record_events" => {
            cfg.gameplay.record_events = crate::social::split_ids(value);
        }
        "event_ttl_days" => {
            cfg.gameplay.event_ttl_days = value.parse().unwrap_or(cfg.gameplay.event_ttl_days);
        }
        "match_history_ttl_days" => {
            cfg.gameplay.match_history_ttl_days =
                value.parse().unwrap_or(cfg.gameplay.match_history_ttl_days);
        }
        "result_event" => cfg.gameplay.result_event = value.to_string(),
        _ => {}
    }
}
