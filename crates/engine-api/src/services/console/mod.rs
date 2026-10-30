use auto_di::singleton;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::repositories::console_mutations::InsertAuditLogParams;
use crate::services::deps::q;

pub mod account;
pub mod audit;
pub mod auth;
pub mod badge;
pub mod board;
pub mod economy;
pub mod flag;
pub mod flag_arms;
pub mod flag_pins;
pub mod flag_rollout;
pub mod group;
pub mod iap;
pub mod leaderboard;
pub mod logs;
#[path = "match.rs"]
pub mod matches;
pub mod mfa;
pub mod notification;
pub mod quest;
pub mod room;
pub mod rules;
pub mod season;
pub mod setting;
pub mod status;
pub mod storage;
pub mod timeline;
pub mod user;

pub struct ConsoleService;

pub(super) const RECOVERY_CODES: usize = 8;

#[singleton]
impl ConsoleService {
    fn new() -> Self {
        Self
    }
}

pub(crate) use crate::utils::ids::{account as account_id, parse_uuid as id};

pub(super) fn never() -> DateTime<Utc> {
    DateTime::from_timestamp(0, 0).expect("the epoch is a valid timestamp")
}

pub(super) async fn audit(
    state: &ApiState,
    claims: &ConsoleAuthed,
    action: &str,
    resource: &str,
    target: Option<Uuid>,
    message: &str,
) {
    insert_audit(
        state,
        claims.0.uuid(),
        &claims.0.username,
        &claims.0.email,
        action,
        resource,
        target,
        message,
    )
    .await;
}

pub(super) async fn insert_audit(
    state: &ApiState,
    id: Option<Uuid>,
    username: &str,
    email: &str,
    action: &str,
    resource: &str,
    target: Option<Uuid>,
    message: &str,
) {
    let Ok(queries) = q(state).await else {
        return;
    };
    let _ = queries
        .insert_audit_log(&InsertAuditLogParams {
            id: Uuid::new_v4(),
            console_user_id: id.unwrap_or_else(Uuid::nil),
            console_username: username.to_owned(),
            email: email.to_owned(),
            action: action.to_owned(),
            resource: resource.to_owned(),
            message: message.to_owned(),
            metadata: match target {
                Some(target) => serde_json::json!({ "target": target }),
                None => serde_json::json!({}),
            },
        })
        .await;
}
