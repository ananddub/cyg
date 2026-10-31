use super::*;

pub(crate) fn wasm_bytes() -> Vec<u8> {
    empty_wasm()
}
pub(crate) const TEST_PREFIX: &str = "engine-api-tests";
pub(crate) fn token_for(role: ConsoleRole) -> String {
    format!(
        "Bearer {}",
        console::mint("", "games-role-probe", "games@console.local", role)
    )
}
pub(crate) fn admin_token() -> String {
    token_for(ConsoleRole::Admin)
}
pub(crate) fn collapse(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned()
}
#[test]
pub(crate) fn the_minted_credentials_keep_their_prefixes_and_fit_their_columns() {
    assert_eq!(GAME_KEY_PREFIX, "gm_");
    assert_eq!(API_KEY_PREFIX, "cyg_sec_");
    assert_eq!(CLIENT_KEY_PREFIX, "cyg_pub_");

    assert_ne!(
        API_KEY_PREFIX, CLIENT_KEY_PREFIX,
        "a secret and a publishable key cannot share a prefix, or a log line cannot \
         tell which leaked"
    );

    const KEY_SLUG_BUDGET: usize = 50;
    const KEY_SUFFIX_LEN: usize = 6;
    assert!(
        GAME_KEY_PREFIX.len() + KEY_SLUG_BUDGET + 1 + KEY_SUFFIX_LEN <= 64,
        "a game key must fit the VARCHAR(64) column"
    );

    for prefix in [API_KEY_PREFIX, CLIENT_KEY_PREFIX] {
        assert!(
            prefix.len() + 64 <= 128,
            "a `{prefix}`-prefixed credential must fit its VARCHAR(128) column"
        );
    }
}
#[test]
pub(crate) fn the_engine_and_status_whitelists_are_the_frozen_ones() {
    let mut engines = ENGINE_TYPES.to_vec();
    engines.sort();
    engines.dedup();
    assert_eq!(
        engines,
        ["custom", "godot", "unity", "unreal", "web"],
        "the registrable engines are the five the plan ships"
    );
    for engine in ["unity", "web", "custom"] {
        assert!(
            ENGINE_TYPES.contains(&engine),
            "`{engine}` must stay a registrable engine"
        );
    }
    assert!(
        !ENGINE_TYPES.contains(&"phaser"),
        "the whitelist is a whitelist: an engine the plane cannot run is refused"
    );

    assert_eq!(
        GAME_STATUSES.as_slice(),
        ["active", "suspended", "archived"]
    );
    assert_eq!(GAME_BACKEND_MODES.as_slice(), ["relay", "wasm"]);
}
#[test]
pub(crate) fn an_engine_decides_its_backend_mode_and_only_web_and_custom_take_wasm() {
    assert_eq!(backend_mode_for_engine("unity"), "relay");
    assert_eq!(backend_mode_for_engine("unreal"), "relay");
    assert_eq!(backend_mode_for_engine("godot"), "relay");
    assert_eq!(backend_mode_for_engine("web"), "wasm");
    assert_eq!(backend_mode_for_engine("custom"), "wasm");

    for engine in ENGINE_TYPES {
        assert!(
            GAME_BACKEND_MODES.contains(&backend_mode_for_engine(engine)),
            "`{engine}` must derive a mode the console knows: {}",
            backend_mode_for_engine(engine)
        );
    }
    assert_eq!(backend_mode_for_engine("unity"), "relay");
}
#[test]
pub(crate) fn the_game_config_document_is_the_room_tuning_of_the_current_plan() {
    let keys = {
        let mut keys: Vec<String> = GameConfig::default()
            .to_value()
            .as_object()
            .expect("a config serializes to an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    };
    assert_eq!(
        keys,
        [
            "auto_close_seconds",
            "max_clients",
            "persistent_rooms",
            "tick_rate"
        ],
        "the config's field list is the plan's four keys"
    );

    let defaults = GameConfig::default().to_value();
    assert_eq!(defaults["tick_rate"], json!(128));
    assert_eq!(defaults["max_clients"], json!(16));
    assert_eq!(defaults["persistent_rooms"], json!(false));
    assert_eq!(defaults["auto_close_seconds"], json!(30));

    let partial: GameConfig =
        serde_json::from_value(json!({"tick_rate": 60})).expect("a partial config deserializes");
    assert_eq!(partial.tick_rate, 60);
    assert_eq!(partial.max_clients, 16);
    assert_eq!(partial.auto_close_seconds, 30);
    assert!(!partial.persistent_rooms);
}
