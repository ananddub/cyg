use crate::services::deps::q;

use super::*;

pub(super) async fn remove_board(state: &ApiState, id: &str) -> Result<(), V2Error> {
    let queries = q(state).await?;
    let removed = queries.delete_board(id).await.map_err(V2Error::from)?;
    if removed.is_empty() {
        return Err(V2Error::not_found(BOARD_NOT_FOUND));
    }
    queries
        .delete_board_records(id)
        .await
        .map_err(V2Error::from)?;
    Ok(())
}
pub(super) async fn audit_board(
    state: &ApiState,
    claims: &ConsoleAuthed,
    action: &str,
    id: &str,
    message: &str,
) {
    super::super::audit(
        state,
        claims,
        action,
        "leaderboard",
        None,
        &format!("{message} {id}"),
    )
    .await;
}
