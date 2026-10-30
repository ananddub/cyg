use axum::Json;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::Page;
use crate::dto::console::group::{GroupExport, GroupMemberRow, GroupRow};
use crate::error::V2Error;
use crate::services::deps::q;

const MAX_MEMBERS: i64 = 1000;

impl super::ConsoleService {
    pub async fn list_groups(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<Vec<GroupRow>>, V2Error> {
        let rows = crate::services::games::tenant::list_groups(
            state,
            page.game_id,
            page.limit(),
            page.offset(),
        )
        .await?;
        Ok(Json(rows.into_iter().map(GroupRow::from).collect()))
    }

    pub async fn get_group(&self, state: &ApiState, id: &str) -> Result<Json<GroupRow>, V2Error> {
        let group = q(state)
            .await?
            .get_group_by_id(super::account_id(id)?)
            .await?;
        Ok(Json(GroupRow::from(group)))
    }

    pub async fn list_group_members(
        &self,
        state: &ApiState,
        id: &str,
    ) -> Result<Json<Vec<GroupMemberRow>>, V2Error> {
        let rows = q(state)
            .await?
            .list_group_users(super::account_id(id)?, MAX_MEMBERS)
            .await?;
        Ok(Json(rows.into_iter().map(GroupMemberRow::from).collect()))
    }

    pub async fn export_group(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<GroupExport>, V2Error> {
        claims.require_write()?;
        let id = super::account_id(id)?;
        let queries = q(state).await?;
        let group = queries.get_group_by_id(id).await?;
        let members = queries.list_group_users(id, MAX_MEMBERS).await?;
        Ok(Json(GroupExport {
            group: GroupRow::from(group),
            members: members.into_iter().map(GroupMemberRow::from).collect(),
        }))
    }

    pub async fn delete_group(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let id = super::account_id(id)?;
        let queries = q(state).await?;
        let group = queries.get_group_by_id(id).await?;
        queries.delete_group(id).await?;
        super::audit(
            state,
            claims,
            "group.delete",
            "group",
            Some(id),
            &format!("Deleted group {}.", group.name),
        )
        .await;
        Ok(Json(Empty {}))
    }
}
