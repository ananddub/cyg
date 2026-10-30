/// `[lifecycle]` block: how long a room lives, idles and holds a seat.
/// Every value here was a literal in the host loop before, so an operator
/// could not trade a fast reap against a long reconnect window.
#[derive(Debug, Clone)]
pub struct LifecycleConfig {
    /// Wall-clock ceiling for one room. `0` = unlimited.
    pub room_max_seconds: u64,
    /// Dispose a room after this long with no players and no spectators.
    pub reap_idle_secs: u64,
    /// Drop a raw UDP peer silent for this long.
    pub peer_idle_ms: u64,
    /// Longest a guest may hold a seat for reconnection, whatever the room
    /// asks for.
    pub seat_hold_secs: u64,
    /// Room names that are never reaped for being empty.
    pub persistent_rooms: Vec<String>,
}

const MAX_WINDOW_SECONDS: u64 = 7 * 24 * 60 * 60;

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            room_max_seconds: 0,
            reap_idle_secs: 30,
            peer_idle_ms: 3_000,
            seat_hold_secs: 120,
            persistent_rooms: Vec::new(),
        }
    }
}

impl LifecycleConfig {
    /// Pull every window into the range the host can act on. A zero would mean
    /// "reap instantly" or "never time out" depending on the site, so the floor
    /// is one second (or 250 ms for a peer) instead.
    pub fn clamp(&mut self) {
        self.room_max_seconds = self.room_max_seconds.min(MAX_WINDOW_SECONDS);
        self.reap_idle_secs = self.reap_idle_secs.clamp(1, MAX_WINDOW_SECONDS);
        self.peer_idle_ms = self.peer_idle_ms.clamp(250, 10 * 60 * 1000);
        self.seat_hold_secs = self.seat_hold_secs.clamp(1, MAX_WINDOW_SECONDS);
        self.persistent_rooms
            .retain(|name| !name.trim().is_empty());
    }

    /// Deadline for a room created at `born`, or `None` when unlimited.
    pub fn closes_at(&self, born: std::time::Instant) -> Option<std::time::Instant> {
        if self.room_max_seconds == 0 {
            return None;
        }
        Some(born + std::time::Duration::from_secs(self.room_max_seconds))
    }

    pub fn is_persistent(&self, room: &str) -> bool {
        self.persistent_rooms.iter().any(|name| name == room)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_windows_the_host_shipped_with() {
        let config = LifecycleConfig::default();
        assert_eq!(config.room_max_seconds, 0);
        assert_eq!(config.reap_idle_secs, 30);
        assert_eq!(config.peer_idle_ms, 3_000);
        assert_eq!(config.seat_hold_secs, 120);
    }

    #[test]
    fn an_out_of_range_window_is_pulled_in_rather_than_trusted() {
        let mut config = LifecycleConfig {
            room_max_seconds: u64::MAX,
            reap_idle_secs: 0,
            peer_idle_ms: 1,
            seat_hold_secs: u64::MAX,
            persistent_rooms: vec!["lobby".into(), "  ".into()],
        };
        config.clamp();
        assert_eq!(config.room_max_seconds, MAX_WINDOW_SECONDS);
        assert_eq!(config.reap_idle_secs, 1);
        assert_eq!(config.peer_idle_ms, 250);
        assert_eq!(config.seat_hold_secs, MAX_WINDOW_SECONDS);
        assert_eq!(config.persistent_rooms, vec!["lobby".to_string()]);
    }

    #[test]
    fn only_a_non_zero_room_length_produces_a_deadline() {
        let born = std::time::Instant::now();
        assert!(LifecycleConfig::default().closes_at(born).is_none());
        let timed = LifecycleConfig {
            room_max_seconds: 90,
            ..LifecycleConfig::default()
        };
        assert_eq!(timed.closes_at(born), Some(born + std::time::Duration::from_secs(90)));
    }
}
