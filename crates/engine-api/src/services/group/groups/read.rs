//! The group read verbs: `GetGroup` and `ListGroups`.

use axum::Json;
use serde_json::json;

use crate::ApiState;
use crate::dto::group::{GroupEnvelope, GroupListResponse, GroupResponse};
use crate::repositories::group::{ListGroupsByNameParams, ListGroupsParams};
use crate::services::auth::q;
use crate::error::V2Error;
use crate::utils::group::group_id;
use crate::app::middleware::Authed;
use crate::app::params::BoolFlag;
use crate::services::social::{self, limit};

use crate::dto::group::ListQuery;

use super::super::fetch_group;
use super::{GROUP_CLOSED, GROUP_OPEN, GroupService};

impl GroupService {
    /// `GET /v2/group/{group_id}` — `apiGetGroup`.
    pub async fn get_group(
        &self,
        state: &ApiState,
        _auth: &Authed,
        raw_id: String,
    ) -> Result<Json<GroupEnvelope>, V2Error> {
        let queries = q(state)?;
        let group = fetch_group(&queries, group_id(&raw_id)?).await?;
        Ok(Json(GroupEnvelope {
            group: GroupResponse::from_row(&group),
        }))
    }

    /// `GET /v2/group?name=&langTag=&open=&members=&limit=&cursor=` — the
    /// `ListGroups`, which searches by name *or* filters by discoverability.
    pub async fn list_groups(
        &self,
        state: &ApiState,
        _auth: &Authed,
        req: ListQuery,
    ) -> Result<Json<GroupListResponse>, V2Error> {
        let queries = q(state)?;
        // the ListGroups defaults to a single group unless asked for more.
        let page = limit(
            req.limit,
            1,
            100,
            "Invalid limit - limit must be between 1 and 100.",
        )?;
        let offset = req
            .cursor
            .as_deref()
            .filter(|c| !c.is_empty())
            .map(|c| {
                social::cursor_decode(c, "Cursor is invalid.")
                    .and_then(|decoded| Ok(social::cursor_i64(&decoded, "offset", 0)))
            })
            .transpose()?
            .unwrap_or(0);
        // The gateway trims a leading run of wildcards and spaces off the pattern,
        // then matches it with ILIKE as written: a client that wants a substring
        // search has to send the `%` itself. The branch is still chosen on the
        // untrimmed value, so a name of only wildcards searches for the empty
        // pattern and finds nothing, exactly as the `ILIKE ''` would.
        let raw_name = req.name.unwrap_or_default();
        let name = raw_name.trim_start_matches(['%', ' ']);
        let lang_tag = req.lang_tag.unwrap_or_default();
        let open = match req.open.as_deref() {
            Some(raw) => Some(matches!(BoolFlag::parse(raw)?, BoolFlag::True)),
            None => None,
        };
        let state = open.map_or(-1, |o| if o { GROUP_OPEN } else { GROUP_CLOSED });
        // The field is an int32, so anything past that range is read as the
        // extreme end of it rather than wrapping into a different filter.
        let members = req
            .members
            .unwrap_or(-1)
            .clamp(i32::MIN as i64, i32::MAX as i64) as i32;

        let rows = if !raw_name.is_empty() {
            if !lang_tag.is_empty() || open.is_some() || members > -1 {
                return Err(V2Error::invalid_argument(
                    "name filter cannot be combined with any other filter",
                ));
            }
            queries
                .list_groups_by_name(&ListGroupsByNameParams {
                    name: name.to_owned(),
                    limit: page + 1,
                    offset,
                })
                .await?
        } else {
            queries
                .list_groups(&ListGroupsParams {
                    lang_tag,
                    state,
                    edge_count: members,
                    limit: page + 1,
                    offset,
                })
                .await?
        };
        let has_more = rows.len() as i64 > page;
        let mut rows = rows;
        if has_more {
            rows.truncate(page as usize);
        }
        let groups: Vec<GroupResponse> = rows.iter().map(GroupResponse::from_row).collect();
        Ok(Json(GroupListResponse {
            groups,
            cursor: if has_more {
                social::cursor_encode(&json!({ "offset": offset + page }))
            } else {
                String::new()
            },
        }))
    }
}
