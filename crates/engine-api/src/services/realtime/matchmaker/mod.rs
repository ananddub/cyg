use auto_di::singleton;
use auto_ws::{Conn, Msg};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

use super::{
    FastMap, FastTree, RealtimeMatchService, RealtimePresenceService, new_fast_map, new_fast_tree,
};
use crate::controllers::realtime::messages::*;
use crate::controllers::realtime::{Dispatched, session_of, unwrap_env_err};
use crate::error::V2Error;
use crate::infra::ws::envelope::{
    EnvelopeMessage, ErrorMsg, MatchmakerAdd, MatchmakerMatched, MatchmakerUser, Presence,
};
use crate::infra::ws::session::{self, Session};
use scc::Guard;

mod numeric_cmp;
mod split_operator;
mod extract_mode;
mod stats;
mod try_match;
mod tests;

pub use numeric_cmp::*;
