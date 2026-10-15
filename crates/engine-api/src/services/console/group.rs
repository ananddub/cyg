//! `/v2/console/group` — groups and their members.

use axum::Json;

use crate::dto::console::Page;
use crate::dto::console::group::{GroupMemberRow, GroupRow};
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
}
