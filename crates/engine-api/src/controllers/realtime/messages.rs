use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use crate::infra::ws::envelope::Presence;

#[derive(Deserialize)]
pub(crate) struct MatchmakerRemove {
    pub(crate) ticket: String,
}

#[derive(Deserialize)]
pub(crate) struct MatchCreate {
    pub(crate) name: Option<String>,
    pub(crate) open: bool,
    pub(crate) tick_rate: i32,
    pub(crate) label: String,
}

#[derive(Deserialize)]
pub(crate) struct MatchJoin {
    pub(crate) match_id: Option<String>,
    pub(crate) token: Option<String>,
    pub(crate) label: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct MatchLeave {
    pub(crate) match_id: String,
}

#[derive(Deserialize)]
pub(crate) struct MatchDataSend {
    pub(crate) match_id: String,
    pub(crate) op_code: i64,
    pub(crate) data: String,
}

#[derive(Deserialize)]
pub(crate) struct MatchSignal {
    pub(crate) match_id: String,
    pub(crate) data: String,
}

#[derive(Deserialize)]
pub(crate) struct PartyCreate {
    pub(crate) max_size: i32,
    pub(crate) open: bool,
}

#[derive(Deserialize)]
pub(crate) struct PartyJoin {
    pub(crate) party_id: String,
}

#[derive(Deserialize)]
pub(crate) struct PartyLeave {
    pub(crate) party_id: String,
}

#[derive(Deserialize)]
pub(crate) struct PartyPromote {
    pub(crate) party_id: String,
    pub(crate) presence: Presence,
}

#[derive(Deserialize)]
pub(crate) struct PartyUpdate {
    pub(crate) party_id: String,
    pub(crate) max_size: Option<i32>,
    pub(crate) open: Option<bool>,
}

#[derive(Deserialize)]
pub(crate) struct PartyClose {
    pub(crate) party_id: String,
    pub(crate) metadata: Value,
}

#[derive(Deserialize)]
pub(crate) struct PartyDataSend {
    pub(crate) party_id: String,
    pub(crate) op_code: i64,
    pub(crate) data: String,
}

#[derive(Deserialize)]
pub(crate) struct PartyMatchmakerAdd {
    pub(crate) min_count: i32,
    pub(crate) max_count: i32,
    pub(crate) query: String,
    pub(crate) string_properties: HashMap<String, String>,
    pub(crate) numeric_properties: HashMap<String, f64>,
}

#[derive(Deserialize)]
pub(crate) struct PartyMatchmakerRemove {
    pub(crate) party_id: String,
    pub(crate) ticket: String,
}

#[derive(Deserialize)]
pub(crate) struct ChannelJoin {
    pub(crate) channel_id: String,
    pub(crate) persistent: bool,
}

#[derive(Deserialize)]
pub(crate) struct ChannelLeave {
    pub(crate) channel_id: String,
}

#[derive(Deserialize)]
pub(crate) struct ChannelMessageSend {
    pub(crate) channel_id: String,
    pub(crate) content: Value,
    pub(crate) persist: Option<bool>,
    pub(crate) ephemeral: Option<bool>,
}

#[derive(Deserialize)]
pub(crate) struct ChannelMessageUpdate {
    pub(crate) channel_id: String,
    pub(crate) message_id: String,
    pub(crate) content: Value,
}

#[derive(Deserialize)]
pub(crate) struct ChannelMessageRemove {
    pub(crate) channel_id: String,
    pub(crate) message_id: String,
}

#[derive(Deserialize)]
pub(crate) struct StreamData {
    pub(crate) stream_id: String,
    pub(crate) data: Value,
    pub(crate) reliable: bool,
}

#[derive(Deserialize)]
pub(crate) struct StatusFollow {
    pub(crate) user_ids: Vec<String>,
    pub(crate) usernames: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct StatusUnfollow {
    pub(crate) user_ids: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct StatusUpdate {
    pub(crate) status: String,
}

#[derive(Deserialize)]
pub(crate) struct NotificationsList {
    pub(crate) limit: Option<i64>,
    pub(crate) cacheable_cursor: Option<String>,
}
