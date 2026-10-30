/// Default retention for mirrored history, in days.
const DEFAULT_EVENT_TTL_DAYS: u32 = 30;

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
        self.event_ttl_days = match self.event_ttl_days {
            0 => DEFAULT_EVENT_TTL_DAYS,
            1..=3650 => self.event_ttl_days,
            _ => 3650,
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
}
