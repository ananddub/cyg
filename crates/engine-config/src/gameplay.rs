const DEFAULT_EVENT_TTL_DAYS: u32 = 30;

const DEFAULT_MATCH_HISTORY_TTL_DAYS: u32 = 90;

const DEFAULT_RESULT_EVENT: &str = "match_result";

fn bounded_ttl(days: u32, default: u32) -> u32 {
    match days {
        0 => default,
        1..=3650 => days,
        _ => 3650,
    }
}

#[derive(Debug, Clone, Default)]
pub struct GameplayConfig {
    pub record_events: Vec<String>,

    pub event_ttl_days: u32,

    pub match_history_ttl_days: u32,

    pub result_event: String,
}

impl GameplayConfig {
    pub fn normalize(&mut self) {
        for name in &mut self.record_events {
            *name = name.trim().to_string();
        }
        self.record_events.retain(|name| !name.is_empty());
        self.record_events.dedup();
        self.event_ttl_days = bounded_ttl(self.event_ttl_days, DEFAULT_EVENT_TTL_DAYS);
        self.match_history_ttl_days =
            bounded_ttl(self.match_history_ttl_days, DEFAULT_MATCH_HISTORY_TTL_DAYS);
        self.result_event = match self.result_event.trim() {
            "" => DEFAULT_RESULT_EVENT.to_string(),
            named => named.to_string(),
        };
    }

    pub fn records(&self, event: &str) -> bool {
        self.record_events.iter().any(|name| name == event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unlisted_event_is_never_recorded() {
        let config = GameplayConfig::default();
        assert!(!config.records("chat"));
        let listed = GameplayConfig {
            record_events: vec!["chat".into(), "score".into()],
            ..Default::default()
        };
        assert!(listed.records("chat"));
        assert!(listed.records("score"));
        assert!(!listed.records("Chat"));
    }

    #[test]
    fn the_list_is_trimmed_and_de_duplicated() {
        let mut config = GameplayConfig {
            record_events: vec![
                " discard ".into(),
                "".into(),
                "discard".into(),
                "deal".into(),
            ],
            ..Default::default()
        };
        config.normalize();
        assert_eq!(config.record_events, ["discard", "deal"]);
        assert!(config.records("discard"));
    }

    #[test]
    fn the_retention_window_always_has_a_usable_length() {
        let mut unset = GameplayConfig::default();
        unset.normalize();
        assert_eq!(unset.event_ttl_days, DEFAULT_EVENT_TTL_DAYS);

        let mut absurd = GameplayConfig {
            event_ttl_days: u32::MAX,
            ..Default::default()
        };
        absurd.normalize();
        assert_eq!(absurd.event_ttl_days, 3650);

        let mut week = GameplayConfig {
            event_ttl_days: 7,
            ..Default::default()
        };
        week.normalize();
        assert_eq!(week.event_ttl_days, 7);
    }

    #[test]
    fn match_history_outlives_the_event_stream_by_default() {
        let mut config = GameplayConfig::default();
        config.normalize();
        assert_eq!(config.event_ttl_days, DEFAULT_EVENT_TTL_DAYS);
        assert_eq!(
            config.match_history_ttl_days,
            DEFAULT_MATCH_HISTORY_TTL_DAYS
        );

        let mut absurd = GameplayConfig {
            match_history_ttl_days: 40_000,
            ..Default::default()
        };
        absurd.normalize();
        assert_eq!(absurd.match_history_ttl_days, 3650);
    }

    #[test]
    fn the_result_event_has_a_name_even_when_the_game_says_nothing() {
        let mut unset = GameplayConfig::default();
        unset.normalize();
        assert_eq!(unset.result_event, DEFAULT_RESULT_EVENT);

        let mut named = GameplayConfig {
            result_event: " game_over ".into(),
            ..Default::default()
        };
        named.normalize();
        assert_eq!(named.result_event, "game_over");
    }
}
