use auto_di::singleton;
use auto_ws::{Conn, Msg};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

use super::{FastMap, new_fast_map};
use crate::ApiState;
use crate::controllers::realtime::messages::*;
use crate::controllers::realtime::{Dispatched, state_of};
use crate::error::V2Error;
use crate::infra::ws::envelope::{Envelope, EnvelopeMessage, Presence, StatusPresence};
use crate::infra::ws::session::{self, Session};

mod realtime_presence_service;
mod to_status_presence;
mod update_status;

pub use realtime_presence_service::*;
