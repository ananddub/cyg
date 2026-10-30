use auto_di::singleton;
use auto_ws::{Conn, Msg};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use super::{FastMap, new_fast_map};
use crate::controllers::realtime::messages::*;
use crate::controllers::realtime::{Dispatched, session_of};
use crate::error::V2Error;
use crate::infra::ws::envelope::{Envelope, EnvelopeMessage, ErrorMsg, Party, Presence};
use crate::infra::ws::session::{self, Session};

mod close;
mod leave;
mod party_state;
mod wire_of;

pub use party_state::*;
