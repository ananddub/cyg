use auto_di::singleton;
use auto_ws::{Conn, Msg};
use chrono::Utc;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use super::{FastMap, new_fast_map};
use crate::ApiState;
use crate::controllers::realtime::messages::*;
use crate::controllers::realtime::{Dispatched, session_of, state_of};
use crate::db::DbError;
use crate::error::V2Error;
use crate::infra::ws::envelope::{
    ChannelMessage, ChannelMessageAck, Envelope, EnvelopeMessage, Presence,
};
use crate::infra::ws::session::{self, Session};
use crate::repositories::channel::{InsertChannelMessageParams, UpdateChannelMessageParams};
use crate::utils::channel::{MODE_DM, MODE_GROUP, Stream, stream_of_channel};

mod channel_state;
mod channel_json;
mod join;
mod message_update;

pub use channel_state::*;
