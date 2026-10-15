//! Groups and their members, as the console lists them.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::repositories::group::ListGroupUsersRow;
use crate::repositories::models::Groups;

/// A group. `state` is the open/closed flag; the console renders the name.
#[derive(Debug, Serialize)]
pub struct GroupRow {
    pub id: Uuid,
    pub creator_id: Uuid,
    pub name: String,
    pub description: String,
    pub avatar_url: String,
    pub lang_tag: String,
    pub metadata: serde_json::Value,
    pub open: bool,
    pub edge_count: i32,
    pub max_count: i32,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

impl From<Groups> for GroupRow {
    fn from(g: Groups) -> Self {
        Self {
            id: g.id,
            creator_id: g.creator_id,
            name: g.name,
            description: g.description.unwrap_or_default(),
            avatar_url: g.avatar_url.unwrap_or_default(),
            lang_tag: g.lang_tag,
            metadata: g.metadata,
            open: g.state == 0,
            edge_count: g.edge_count,
            max_count: g.max_count,
            create_time: g.create_time,
            update_time: g.update_time,
        }
    }
}

/// One group member, with the group-edge state the member sits at.
#[derive(Debug, Serialize)]
pub struct GroupMemberRow {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_url: String,
    pub metadata: serde_json::Value,
    /// The group-edge state name: `superadmin`, `admin`, `member`, `join_request` or `banned`.
    pub state: &'static str,
    pub create_time: DateTime<Utc>,
    pub edge_update_time: Option<DateTime<Utc>>,
}

impl From<ListGroupUsersRow> for GroupMemberRow {
    fn from(m: ListGroupUsersRow) -> Self {
        Self {
            id: m.id,
            username: m.username,
            display_name: m.display_name.unwrap_or_default(),
            avatar_url: m.avatar_url.unwrap_or_default(),
            metadata: m.metadata,
            state: state_name(m.state.unwrap_or(2)),
            create_time: m.create_time,
            edge_update_time: m.edge_update_time,
        }
    }
}

/// The `group_edge.state` names, in schema order.
fn state_name(state: i16) -> &'static str {
    match state {
        0 => "superadmin",
        1 => "admin",
        2 => "member",
        3 => "join_request",
        4 => "banned",
        _ => "member",
    }
}
