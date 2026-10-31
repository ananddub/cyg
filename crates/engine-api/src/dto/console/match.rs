use serde::Serialize;

use crate::dto::matches::MatchResponse;
use crate::infra::ws::envelope::Presence;
use crate::services::realtime::matches::MatchSnapshot;

#[derive(Debug, Serialize)]
pub struct ConsoleMatchRow {
    pub api_match: MatchResponse,
    pub node: String,
}

impl From<&MatchSnapshot> for ConsoleMatchRow {
    fn from(m: &MatchSnapshot) -> Self {
        Self {
            api_match: MatchResponse::from(m),
            node: String::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ConsoleMatchList {
    pub matches: Vec<ConsoleMatchRow>,
}

#[derive(Debug, Serialize)]
pub struct ConsoleMatchState {
    pub presences: Vec<Presence>,
    pub tick: i64,
    pub state: String,
}
