use super::*;

pub struct Match {
    pub id: Uuid,
    pub label: String,
    pub open: bool,
    pub tick_rate: i32,
    pub members: HashMap<Uuid , Presence>,
    pub tokens: HashSet<String>,
    pub ephemeral: bool,
}
pub struct MatchSnapshot {
    pub id: Uuid,
    pub label: String,
    pub open: bool,
    pub tick_rate: i32,
    pub members: Vec<Presence>,
}
pub struct RealtimeMatchService {
    pub matches: &'static FastMap<Uuid, Match>,
    pub tokens_to_match: &'static FastMap<String, Uuid>,
}
#[singleton]
impl RealtimeMatchService {
    pub fn new() -> Self {
        static MATCHES: OnceLock<FastMap<Uuid, Match>> = OnceLock::new();
        static TOKENS: OnceLock<FastMap<String, Uuid>> = OnceLock::new();
        Self {
            matches: MATCHES.get_or_init(new_fast_map),
            tokens_to_match: TOKENS.get_or_init(new_fast_map),
        }
    }

    pub fn global() -> &'static RealtimeMatchService {
        static INSTANCE: OnceLock<RealtimeMatchService> = OnceLock::new();
        INSTANCE.get_or_init(RealtimeMatchService::new)
    }

    pub fn list_snapshots_all() -> Vec<MatchSnapshot> {
        Self::global().list_snapshots()
    }

    pub fn get_snapshot(id: Uuid) -> Option<MatchSnapshot> {
        Self::global().snapshot(id)
    }

    pub fn check_match_exists(id: Uuid) -> bool {
        Self::global().match_exists(id)
    }

    pub fn create_match_with_tokens(tick_rate: i32, tokens: Vec<String>) -> Uuid {
        Self::global().create_with_tokens(tick_rate, tokens)
    }

    pub fn terminate_match(id: Uuid) -> bool {
        Self::global().terminate(id)
    }
}
