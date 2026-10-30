/// Default retention for mirrored history, in days.
const DEFAULT_EVENT_TTL_DAYS: u32 = 30;
/// Default retention for finished matches, in days.
const DEFAULT_MATCH_HISTORY_TTL_DAYS: u32 = 90;
/// The guest event whose payload the host files as the match's outcome. A game
/// that names its own result event sets `result_event`; until then this is the
/// convention the SDK docs use.
const DEFAULT_RESULT_EVENT: &str = "match_result";

/// A retention window an operator can actually write: `0` is "use the default"
/// and a decade is a typo, not a policy.
fn bounded_ttl(days: u32, default: u32) -> u32 {
    match days {
        0 => default,
        1..=3650 => days,
        _ => 3650,
    }
}

/// `[gameplay]` block: which guest `host_send` messages the engine mirrors to
/// the control plane for persistence. Empty by default — an unlisted event is
/// wire-only, so a chatty game cannot fill the database by accident, and the
/// names are the game's own vocabulary rather than an engine enum.
#[derive(Debug, Clone, Default)]
pub struct GameplayConfig {
    /// Event names to record, matched exactly against the `host_send` name.
    /// `desync` is the engine's own name and is recorded whether or not it is
    /// listed here.
    pub record_events: Vec<String>,
    /// Days a mirrored event or desync row stays queryable. `0` means the
    /// default; the control plane clamps the rest, so a typo cannot delete the
    /// history a game depends on or keep it forever.
    pub event_ttl_days: u32,
    /// Days a finished match stays in `match_history`. Match rows are one per
    /// room and tiny next to the event stream, so they outlive it by default.
    pub match_history_ttl_days: u32,
    /// Event name whose last payload becomes the match row's `result`, so a
    /// history listing shows who won without replaying the room.
    pub result_event: String,
}

impl GameplayConfig {
    /// Trim, drop blanks and de-duplicate: the list is read on every outbound
    /// message, so a name written twice would mirror the event twice.
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

    /// Whether one outbound event belongs in the history table.
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
