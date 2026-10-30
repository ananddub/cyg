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
    /// How long the host keeps simulating after SIGTERM, so peers are told to
    /// leave and their state is saved instead of the process simply stopping.
    pub drain_grace_secs: u64,
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
            drain_grace_secs: 15,
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
        self.persistent_rooms.retain(|name| !name.trim().is_empty());
        // A drain longer than the pod's termination deadline would be cut off
        // anyway, so the ceiling is the widest grace period the manifests ask for.
        self.drain_grace_secs = self.drain_grace_secs.clamp(1, 300);
    }

    /// How long a raw UDP peer may go silent before the plane drops it.
    pub fn peer_idle(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.peer_idle_ms)
    }

    /// Read the windows from the environment on top of the shipped defaults:
    /// the pinned sim plane is configured by env alone, and it must honour the
    /// same peer-idle window the host loop does.
    pub fn from_env() -> LifecycleConfig {
        let mut lifecycle = LifecycleConfig::default();
        lifecycle.apply_env();
        lifecycle.clamp();
        lifecycle
    }

    pub fn apply_env(&mut self) {
        for (name, field) in [
            ("OX_ROOM_MAX_SECONDS", &mut self.room_max_seconds),
            ("OX_REAP_IDLE_SECS", &mut self.reap_idle_secs),
            ("OX_PEER_IDLE_MS", &mut self.peer_idle_ms),
            ("OX_SEAT_HOLD_SECS", &mut self.seat_hold_secs),
            ("OX_DRAIN_GRACE_SECS", &mut self.drain_grace_secs),
        ] {
            if let Ok(value) = std::env::var(name)
                && let Ok(parsed) = value.parse()
            {
                *field = parsed;
            }
        }
        if let Ok(value) = std::env::var("OX_PERSISTENT_ROOMS") {
            self.persistent_rooms = crate::social::split_ids(&value);
        }
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
        assert_eq!(config.drain_grace_secs, 15);
    }

    #[test]
    fn an_out_of_range_window_is_pulled_in_rather_than_trusted() {
        let mut config = LifecycleConfig {
            room_max_seconds: u64::MAX,
            reap_idle_secs: 0,
            peer_idle_ms: 1,
            seat_hold_secs: u64::MAX,
            persistent_rooms: vec!["lobby".into(), "  ".into()],
            drain_grace_secs: 0,
        };
        config.clamp();
        assert_eq!(config.room_max_seconds, MAX_WINDOW_SECONDS);
        assert_eq!(config.reap_idle_secs, 1);
        assert_eq!(config.peer_idle_ms, 250);
        assert_eq!(config.seat_hold_secs, MAX_WINDOW_SECONDS);
        assert_eq!(config.persistent_rooms, vec!["lobby".to_string()]);
        assert_eq!(config.drain_grace_secs, 1);
    }

    #[test]
    fn only_a_non_zero_room_length_produces_a_deadline() {
        let born = std::time::Instant::now();
        assert!(LifecycleConfig::default().closes_at(born).is_none());
        let timed = LifecycleConfig {
            room_max_seconds: 90,
            ..LifecycleConfig::default()
        };
        assert_eq!(
            timed.closes_at(born),
            Some(born + std::time::Duration::from_secs(90))
        );
    }
}
