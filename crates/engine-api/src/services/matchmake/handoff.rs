//! The registry's answer to "may this node run that room?".
//!
//! A seat minted here means a room spawned here: the host creates the room on
//! the peer's first packet, so two nodes holding the same name would run two
//! rooms and the client would join whichever answered. The claim a running host
//! publishes is what stops that, and the lease below is what stops it forever.

use axum::http::StatusCode;
use chrono::Utc;

use crate::ApiError;
use crate::ApiState;
use crate::repositories::room_registry::lease_lapsed;
use crate::services::deps::q;
use crate::utils::matchmake::{MATCHMAKE_METHODS, printable, valid_room_name};

/// Refuse a room that another node runs, for the methods that would spawn it
/// here. Anything else is allowed, including every lookup failure: this gate
/// protects against a second copy of a live room, and a datastore that is
/// unreachable is not that.
pub async fn guard(state: &ApiState, method: &str, room: &str) -> Result<(), ApiError> {
    // `reconnect` and `rejoin` carry a token the owner minted. Every node shares
    // the seat secret, so that ticket *is* the handoff: it verifies wherever the
    // client presents it, and refusing it here would strand the player instead
    // of reattaching them.
    if matches!(method, "reconnect" | "rejoin")
        || !MATCHMAKE_METHODS.contains(&method)
        || !valid_room_name(room)
    {
        return Ok(());
    }

    let Ok(queries) = q(state).await else {
        return Ok(());
    };
    let row = match queries.load_room_registry(room).await {
        Ok(Some(row)) => row,
        Ok(None) => return Ok(()),
        Err(err) => {
            tracing::warn!("[control] room registry read for '{room}' failed: {err}");
            return Ok(());
        }
    };

    if row.node_id == engine_config::node_id() {
        return Ok(());
    }
    if lease_lapsed(&row, Utc::now()) {
        // The owner stopped refreshing: its pod is gone, and this node is now the
        // one that can make the room. A name whose owner died must not be
        // un-joinable until somebody deletes a row.
        return Ok(());
    }
    Err(ApiError::new(
        StatusCode::CONFLICT,
        format!(
            "room '{}' is running on node '{}'; attach there",
            printable(room),
            printable(&row.node_id)
        ),
    ))
}
