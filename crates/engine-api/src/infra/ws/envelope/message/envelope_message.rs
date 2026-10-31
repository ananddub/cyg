use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EnvelopeMessage {
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "error")]
    Error(ErrorMsg),

    #[serde(rename = "matchmaker_add")]
    MatchmakerAdd(MatchmakerAdd),
    #[serde(rename = "matchmaker_remove")]
    MatchmakerRemove { ticket: String },
    #[serde(rename = "matchmaker_ticket")]
    MatchmakerTicket { ticket: String },
    #[serde(rename = "matchmaker_matched")]
    MatchmakerMatched(MatchmakerMatched),

    #[serde(rename = "match_create")]
    MatchCreate {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        open: bool,
        #[serde(default)]
        tick_rate: i32,
        #[serde(default)]
        label: String,
        #[serde(default)]
        min_size: Option<i32>,
    },
    #[serde(rename = "match_join")]
    MatchJoin {
        #[serde(default)]
        match_id: Option<String>,
        #[serde(default)]
        token: Option<String>,
        #[serde(default)]
        label: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        presences: Vec<Presence>,
    },
    #[serde(rename = "match_create_ack")]
    MatchCreateAck {
        #[serde(default)]
        match_id: String,
    },
    #[serde(rename = "match_leave")]
    MatchLeave { match_id: String },
    #[serde(rename = "match_data_send")]
    MatchDataSend {
        match_id: String,
        #[serde(default)]
        op_code: i64,
        data: String,
    },
    #[serde(rename = "match_data")]
    MatchData {
        match_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        presence: Option<Presence>,
        #[serde(default)]
        op_code: i64,
        data: String,
        #[serde(default)]
        reliable: bool,
    },
    #[serde(rename = "match_presence_event")]
    MatchPresenceEvent {
        match_id: String,
        #[serde(default)]
        joins: Vec<Presence>,
        #[serde(default)]
        leaves: Vec<Presence>,
    },
    #[serde(rename = "match_signal")]
    MatchSignal { match_id: String, data: String },
    #[serde(rename = "matchmaker_done")]
    MatchmakerDone,

    #[serde(rename = "party_create")]
    PartyCreate {
        #[serde(default)]
        max_size: i32,
        #[serde(default = "default_open")]
        open: bool,
    },
    #[serde(rename = "party")]
    PartyReply { party: Party },
    #[serde(rename = "party_join")]
    PartyJoin { party_id: String },
    #[serde(rename = "party_leave")]
    PartyLeave {
        party_id: String,
        #[serde(default)]
        metadata: Value,
    },
    #[serde(rename = "party_promote")]
    PartyPromote {
        party_id: String,
        presence: Presence,
    },
    #[serde(rename = "party_update")]
    PartyUpdate {
        party_id: String,
        #[serde(default)]
        max_size: Option<i32>,
        #[serde(default)]
        open: Option<bool>,
    },
    #[serde(rename = "party_close")]
    PartyClose {
        party_id: String,
        #[serde(default)]
        metadata: Value,
    },
    #[serde(rename = "party_data_send")]
    PartyDataSend {
        party_id: String,
        #[serde(default)]
        op_code: i64,
        data: String,
    },
    #[serde(rename = "party_data")]
    PartyData {
        party_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        presence: Option<Presence>,
        #[serde(default)]
        op_code: i64,
        data: String,
    },
    #[serde(rename = "party_member_join")]
    PartyMemberJoin {
        party_id: String,
        joins: Vec<Presence>,
    },
    #[serde(rename = "party_member_leave")]
    PartyMemberLeave {
        party_id: String,
        leaves: Vec<Presence>,
    },
    #[serde(rename = "party_matchmaker_add")]
    PartyMatchmakerAdd {
        min_count: i32,
        max_count: i32,
        #[serde(default)]
        query: String,
        #[serde(default)]
        string_properties: HashMap<String, String>,
        #[serde(default)]
        numeric_properties: HashMap<String, f64>,
    },
    #[serde(rename = "party_matchmaker_remove")]
    PartyMatchmakerRemove { party_id: String, ticket: String },
    #[serde(rename = "party_matchmaker_ticket")]
    PartyMatchmakerTicket { party_id: String, ticket: String },
    #[serde(rename = "party_signal")]
    PartySignal { party_id: String },

    #[serde(rename = "channel_join")]
    ChannelJoin {
        channel_id: String,
        #[serde(default)]
        persistent: bool,
    },
    #[serde(rename = "channel")]
    ChannelReply {
        channel: Value,
        #[serde(default)]
        presences: Vec<Presence>,
    },
    #[serde(rename = "channel_leave")]
    ChannelLeave { channel_id: String },
    #[serde(rename = "channel_message_send")]
    ChannelMessageSend {
        channel_id: String,
        content: Value,
        #[serde(default)]
        persist: Option<bool>,
        #[serde(default)]
        ephemeral: Option<bool>,
    },
    #[serde(rename = "channel_message_update")]
    ChannelMessageUpdate {
        channel_id: String,
        message_id: String,
        content: Value,
    },
    #[serde(rename = "channel_message_remove")]
    ChannelMessageRemove {
        channel_id: String,
        message_id: String,
    },
    #[serde(rename = "channel_message")]
    ChannelMessage(ChannelMessage),
    #[serde(rename = "channel_message_ack")]
    ChannelMessageAck(ChannelMessageAck),
    #[serde(rename = "channel_presence_event")]
    ChannelPresenceEvent {
        channel_id: String,
        #[serde(default)]
        joins: Vec<Presence>,
        #[serde(default)]
        leaves: Vec<Presence>,
    },

    #[serde(rename = "status_follow")]
    StatusFollow {
        #[serde(default)]
        user_ids: Vec<String>,
        #[serde(default)]
        usernames: Vec<String>,
    },
    #[serde(rename = "status_followed")]
    StatusFollowed { user_ids: Vec<String> },
    #[serde(rename = "status_unfollow")]
    StatusUnfollow { user_ids: Vec<String> },
    #[serde(rename = "status_update")]
    StatusUpdate { status: String },
    #[serde(rename = "status_presence_event")]
    StatusPresenceEvent {
        #[serde(default)]
        joins: Vec<StatusPresence>,
        #[serde(default)]
        leaves: Vec<StatusPresence>,
    },

    #[serde(rename = "stream_data")]
    StreamData {
        stream_id: String,
        data: Value,
        #[serde(default)]
        reliable: bool,
    },
    #[serde(rename = "notifications")]
    NotificationsList {
        #[serde(default)]
        limit: Option<i64>,
        #[serde(default)]
        cacheable_cursor: Option<String>,
    },
    #[serde(rename = "notifications_list")]
    Notifications {
        notifications: Vec<NotificationResponse>,
        #[serde(default)]
        cacheable_cursor: String,
    },
    #[serde(rename = "notification")]
    NotificationEvent { notification: NotificationResponse },
}
