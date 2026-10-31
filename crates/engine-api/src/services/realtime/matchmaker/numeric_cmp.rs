use super::*;

pub(super) static TICKER_STARTED: OnceLock<()> = OnceLock::new();
#[derive(Clone, Debug, PartialEq)]
pub enum NumericCmp {
    Gte(f64),
    Lte(f64),
    Gt(f64),
    Lt(f64),
    Eq(f64),
}
#[derive(Clone, Debug, PartialEq)]
pub enum TermMatcher {
    Numeric(NumericCmp),
    Exact(String),
}
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledTerm {
    pub key: String,
    pub negated: bool,
    pub matcher: TermMatcher,
}
#[derive(Clone)]
pub struct Ticket {
    pub id: Uuid,
    pub sid: Uuid,
    pub uid: Uuid,
    pub username: String,
    pub min_count: i32,
    pub max_count: i32,
    pub query: String,
    pub compiled_terms: Vec<CompiledTerm>,
    pub string_props: HashMap<String, String>,
    pub numeric_props: HashMap<String, f64>,
    pub party_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
pub struct MatchsetStats {
    pub matchset: String,
    pub ticket_count: i64,
    pub oldest_create_time: DateTime<Utc>,
    pub avg_wait_secs: f64,
}
pub struct MatchmakerStatsSnapshot {
    pub ticket_count: i64,
    pub oldest_ticket_create_time: Option<DateTime<Utc>>,
    pub matchsets: Vec<MatchsetStats>,
}
pub struct RealtimeMatchmakerService {
    pub tickets: &'static FastMap<Uuid, Ticket>,
    pub mode_index: &'static FastMap<String, FastMap<Uuid, ()>>,
    pub rank_index: &'static FastTree<(i64, Uuid), ()>,
}
#[singleton]
impl RealtimeMatchmakerService {
    pub fn new() -> Self {
        static TICKETS: OnceLock<FastMap<Uuid, Ticket>> = OnceLock::new();
        static MODE_INDEX: OnceLock<FastMap<String, FastMap<Uuid, ()>>> = OnceLock::new();
        static RANK_INDEX: OnceLock<FastTree<(i64, Uuid), ()>> = OnceLock::new();
        Self {
            tickets: TICKETS.get_or_init(new_fast_map),
            mode_index: MODE_INDEX.get_or_init(new_fast_map),
            rank_index: RANK_INDEX.get_or_init(new_fast_tree),
        }
    }

    pub fn global() -> &'static RealtimeMatchmakerService {
        static INSTANCE: OnceLock<RealtimeMatchmakerService> = OnceLock::new();
        INSTANCE.get_or_init(RealtimeMatchmakerService::new)
    }

    pub fn add_ticket(
        s: &Arc<Session>,
        min_count: i32,
        max_count: i32,
        query: String,
        string_props: HashMap<String, String>,
        numeric_props: HashMap<String, f64>,
    ) -> Result<String, EnvelopeMessage> {
        Self::global().add_ticket_internal(
            s,
            min_count,
            max_count,
            query,
            string_props,
            numeric_props,
        )
    }

    pub fn remove_ticket(s: &Arc<Session>, ticket: &str) -> bool {
        Self::global().remove_ticket_internal(s, ticket)
    }

    pub fn drop_all_tickets(sid: Uuid) {
        Self::global().drop_tickets(sid);
    }

    pub fn get_stats() -> MatchmakerStatsSnapshot {
        Self::global().stats()
    }
}
