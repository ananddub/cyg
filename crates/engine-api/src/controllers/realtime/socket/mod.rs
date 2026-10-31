use std::sync::Arc;

use auto_ws::{Conn, Msg, auto_ws};

use crate::infra::ws::envelope::EnvelopeMessage;
use crate::infra::ws::session;

use super::Dispatched;
use crate::services::realtime::{
    RealtimeChatService, RealtimeMatchService, RealtimeMatchmakerService,
    RealtimeNotificationService, RealtimePartyService, RealtimePresenceService, RealtimeSessionHub,
};

mod controller;
mod routes;

use controller::*;
