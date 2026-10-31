use auto_di::singleton;
use auto_ws::{Conn, Msg};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

use super::{FastMap, new_fast_map};
use crate::controllers::realtime::messages::*;
use crate::controllers::realtime::{Dispatched, session_of};
use crate::error::V2Error;
use crate::infra::ws::envelope::{Envelope, EnvelopeMessage, Presence};
use crate::infra::ws::session::{self, Session};

mod match_state;
mod snapshot_of;
mod join_match;
mod data_send_match;
mod create;

pub use match_state::*;
