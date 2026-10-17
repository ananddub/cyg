//! The admin console: its own authentication realm, the views an operator browses
//! and the writes an operator makes.
//!
//! One service type with a module per area — the shape
//! [`crate::services::account`] uses — so a controller injects a single service
//! and reaches every area method, and an area can split once it grows.

use auto_di::singleton;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::app::middleware::ConsoleAuthed;
use crate::error::V2Error;
use crate::repositories::console_mutations::InsertAuditLogParams;
use crate::repositories::db::Queries;
use crate::ApiState;

pub mod account;
pub mod audit;
pub mod auth;
pub mod board;
pub mod group;
pub mod iap;
pub mod leaderboard;
#[path = "match.rs"]
pub mod matches;
pub mod mfa;
pub mod notification;
pub mod setting;
pub mod statements;
pub mod status;
pub mod storage;
pub mod user;

pub struct ConsoleService;

/// How many one-time codes an MFA enrolment hands out. Eight is what fits on one
/// page for an operator to write down, and what a lost device needs.
pub(super) const RECOVERY_CODES: usize = 8;

#[singleton]
impl ConsoleService {
    fn new() -> Self {
        Self
    }
}

/// The console is entirely database-backed: without a pool every view answers 503
/// rather than pretending a table is empty.
pub(super) fn queries(state: &ApiState) -> Result<std::sync::Arc<Queries>, V2Error> {
    state.q.clone().ok_or_else(V2Error::no_db)
}

/// The pool the hand-written console statements run against, or 503 when the
/// database is not configured.
pub(super) fn pool(state: &ApiState) -> Result<&sqlx::PgPool, V2Error> {
    state.db.as_ref().ok_or_else(V2Error::no_db)
}

/// Parse a path id, naming the thing that was malformed so the refusal reads the
/// way the rest of the API's do.
pub(super) fn id(raw: &str, what: &str) -> Result<Uuid, V2Error> {
    Uuid::parse_str(raw).map_err(|_| V2Error::invalid_argument(format!("Invalid {what} id.")))
}

/// Parse a player account id.
pub(super) fn account_id(raw: &str) -> Result<Uuid, V2Error> {
    id(raw, "account")
}

/// The instant the schema stores for "never happened" — an unbanned account, an
/// unrefunded purchase. Every comparison against it is an equality, so the value
/// has to be exactly this.
pub(super) fn never() -> DateTime<Utc> {
    DateTime::from_timestamp(0, 0).expect("the epoch is a valid timestamp")
}

/// Append a console action to the audit trail, best effort. An operator must not be
/// blocked — or a write rolled back — because the trail could not be written, so a
/// missing database and a failed insert are both silent here.
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

/// The write behind [`audit`], for the sign-in paths that have no verified token
/// yet, or whose token is what is being revoked.
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
    let Some(queries) = state.q.clone() else {
        return;
    };
    let _ = queries
        .insert_audit_log(&InsertAuditLogParams {
            id: Uuid::new_v4(),
            // The bootstrap superadmin has no row; the nil id is how the trail
            // tells it apart from a real operator.
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
