use axum::Json;

use crate::ApiState;
use crate::dto::console::room::{ConsoleRoomList, ConsoleRoomRow};
use crate::error::V2Error;
use crate::services::deps::q;

impl super::ConsoleService {
    /// Every room the hosts have claimed, whichever node runs it. The list comes
    /// straight off the registry rather than the in-process room table, so a
    /// fleet's operator sees the whole fleet and not one pod.
    pub async fn list_rooms(&self, state: &ApiState) -> Result<Json<ConsoleRoomList>, V2Error> {
        let rows = q(state).await?.list_room_registry().await?;
        Ok(Json(ConsoleRoomList {
            rooms: rows.into_iter().map(ConsoleRoomRow::from_registry).collect(),
        }))
    }
}
