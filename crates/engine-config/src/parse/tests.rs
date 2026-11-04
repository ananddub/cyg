use super::*;

#[test]
fn wasm_pooling_defaults_on_and_parses() {
    let cfg = parse_config("tick_rate: 60\n");
    assert!(cfg.wasm_pooling, "pooling is the default — bounded guests pool slots");
    assert_eq!(cfg.wasm_max_memory_mb, 64);

    let cfg = parse_config("wasm_pooling: false\nwasm_max_memory_mb: 128\n");
    assert!(!cfg.wasm_pooling);
    assert_eq!(cfg.wasm_max_memory_mb, 128);

    let toml = parse_config("wasm_pooling = false\nwasm_max_memory_mb = 256\n");
    assert!(!toml.wasm_pooling);
    assert_eq!(toml.wasm_max_memory_mb, 256);
}

#[test]
fn patch_rate_parses_and_defaults_to_uncapped() {
    let cfg = parse_config("tick_rate: 60\npatch_rate: 20\n");
    assert_eq!(cfg.tick_rate, 60);
    assert_eq!(cfg.patch_rate, 20);
    assert_eq!(cfg.patch_rate_ms(), 50);

    let cfg = parse_config("tick_rate: 60\n");
    assert_eq!(cfg.patch_rate, 0, "unset patch_rate follows the guest");
    assert_eq!(cfg.patch_rate_ms(), 0);

    let cfg = parse_config("patch_rate: -5\n");
    assert_eq!(cfg.patch_rate, 0, "negative patch_rate clamps to uncapped");

    let toml = parse_config(
        r#"
tick_rate = 128
patch_rate = 10
"#,
    );
    assert_eq!(toml.patch_rate, 10);
    assert_eq!(toml.patch_rate_ms(), 100);
}

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
