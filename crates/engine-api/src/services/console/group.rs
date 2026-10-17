//! `/v2/console/group` — groups, their members, and deleting a group.

use axum::Json;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::Page;
use crate::dto::console::group::{GroupExport, GroupMemberRow, GroupRow};
use crate::error::V2Error;
use crate::repositories::group::ListGroupsParams;
use crate::ApiState;

/// The console lists every group: no language, state or size filter is applied,
/// which the `-1`/`""` bounds the statement reads as "unbounded" express.
const ANY: (&str, i16, i32) = ("", -1, -1);

/// A group has at most a few thousand members; the console caps the member page so
/// a hostile group cannot make the dashboard fetch a whole table.
const MAX_MEMBERS: i64 = 1000;

impl super::ConsoleService {
    pub async fn list_groups(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<Vec<GroupRow>>, V2Error> {
        let (lang_tag, state_filter, edge_count) = ANY;
        let rows = super::queries(state)?
            .list_groups(&ListGroupsParams {
                lang_tag: lang_tag.to_owned(),
                state: state_filter,
                edge_count,
                limit: page.limit(),
                offset: page.offset(),
            })
            .await?;
        Ok(Json(rows.into_iter().map(GroupRow::from).collect()))
    }

    pub async fn get_group(
        &self,
        state: &ApiState,
        id: &str,
    ) -> Result<Json<GroupRow>, V2Error> {
        let group = super::queries(state)?
            .get_group_by_id(super::account_id(id)?)
            .await?;
        Ok(Json(GroupRow::from(group)))
    }

    pub async fn list_group_members(
        &self,
        state: &ApiState,
        id: &str,
    ) -> Result<Json<Vec<GroupMemberRow>>, V2Error> {
        let rows = super::queries(state)?
            .list_group_users(super::account_id(id)?, MAX_MEMBERS)
            .await?;
        Ok(Json(rows.into_iter().map(GroupMemberRow::from).collect()))
    }

    /// One group whole: its row and its members. Reading an export is a write-grade
    /// privilege, the same as an account export, because it carries the member list
    /// and the metadata alongside it.
    pub async fn export_group(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<GroupExport>, V2Error> {
        claims.require_write()?;
        let id = super::account_id(id)?;
        let queries = super::queries(state)?;
        let group = queries.get_group_by_id(id).await?;
        let members = queries.list_group_users(id, MAX_MEMBERS).await?;
        Ok(Json(GroupExport {
            group: GroupRow::from(group),
            members: members.into_iter().map(GroupMemberRow::from).collect(),
        }))
    }

    /// Delete a group. This is the same statement the player API uses, which is a
    /// soft delete: the row keeps its history, stops being listed, and every other
    /// surface reads that epoch as "gone". A hard delete here would be a second
    /// meaning for the word the API already defines.
    pub async fn delete_group(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let id = super::account_id(id)?;
        let queries = super::queries(state)?;
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
