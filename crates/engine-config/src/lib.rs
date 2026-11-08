mod console;
mod control;
mod game;
mod gameplay;
mod iap;
mod lifecycle;
mod maps;
mod module;
mod netcode;

pub mod node;
mod parse;

pub mod quota;
mod rotation;
mod secret_audit;
mod session;
mod social;
mod templates;
mod watchdog;

pub use console::ConsoleConfig;
pub use control::{AgonesConfig, ControlConfig};
pub use game::{
    DEV_SEAT_SECRET, FeaturesConfig, GameConfig, PhysicsFeature, RoomVariantDef, Runtime,
};
pub use gameplay::GameplayConfig;
pub use iap::IapConfig;
pub use lifecycle::LifecycleConfig;
pub use maps::{BootMaps, MapAsset, MapFormat};
pub use module::BootModule;
pub use netcode::{AoiConfig, RollbackConfig};
pub use node::node_id;
pub use parse::{load_config, parse_config};
pub use rotation::{CURRENT_KEY_ID, PREVIOUS_KEY_ID, Rotation};
pub use secret_audit::{Check, audit_secrets, placeholder_secret, weak_secret};
pub use session::SessionConfig;
pub use social::SocialConfig;
pub use templates::{CONFIG_TEMPLATE, ROOM_TEMPLATE, SCHEMA_TS_TEMPLATE};
pub use watchdog::WatchdogConfig;

#[cfg(test)]
mod room_variants_tests {
    use super::*;

    #[test]
    fn rooms_tables_parse_into_variants() {
        let cfg = crate::parse_config(
            r#"
            name = "t"
            [rooms.duo]
            filter_by = ["region"]
            max_clients = 2
            [rooms.ranked]
            filter_by = ["region", "skill"]
            options = '{ "ranked": true }'
            "#,
        );
        assert_eq!(cfg.rooms.len(), 2);
        let duo = cfg.rooms.get("duo").unwrap();
        assert_eq!(duo.filter_by, vec!["region".to_string()]);
        assert_eq!(duo.max_clients, Some(2));
        let ranked = cfg.rooms.get("ranked").unwrap();
        assert_eq!(ranked.options.as_deref(), Some("{ \"ranked\": true }"));
    }

    #[test]
    fn room_key_suffix_groups_equal_filters_and_splits_others() {
        let cfg = crate::parse_config("[rooms.duo]\nfilter_by = [\"region\", \"skill\"]\n");
        let duo = cfg.rooms.get("duo").unwrap();

        let a = duo.room_key_suffix(r#"{"region":"mumbai","skill":5}"#);
        let b = duo.room_key_suffix(r#"{"skill":5,"region":"mumbai"}"#);
        assert_eq!(a, b, "key order must not matter");

        let c = duo.room_key_suffix(r#"{"region":"delhi","skill":5}"#);
        assert_ne!(a, c, "different region = different room");

        // missing keys produce no suffix terms — both empty-filter joins share one room
        let empty = duo.room_key_suffix("{}");
        assert!(!empty.contains("region"));
    }
}
