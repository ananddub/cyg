use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dto::notification::NotificationResponse;

use super::chat::{ChannelMessage, ChannelMessageAck};
use super::error::ErrorMsg;
use super::matchmaker::{MatchmakerAdd, MatchmakerMatched};
use super::party::Party;
use super::presence::{Presence, StatusPresence};

mod default_open;
mod envelope_message;

use default_open::*;
pub use envelope_message::*;
