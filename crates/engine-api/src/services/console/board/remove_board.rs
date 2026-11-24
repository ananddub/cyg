use crate::services::plane::Plane;

use super::*;

pub(super) async fn remove_board(plane: &Plane, id: &str) -> Result<(), V2Error> {
    let queries = plane.q().await?;
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
    plane: &Plane,
    claims: &ConsoleAuthed,
    action: &str,
    id: &str,
    message: &str,
) {
    super::super::audit(
        plane,
        claims,
        action,
        "leaderboard",
        None,
        &format!("{message} {id}"),
    )
    .await;
}
