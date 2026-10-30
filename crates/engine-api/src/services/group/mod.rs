use uuid::Uuid;

use crate::db::DbError;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::models::Groups;

pub mod groups;
pub mod members;
pub mod user_groups;

pub use groups::GroupService;
pub use members::GroupMemberService;
pub use user_groups::UserGroupService;

pub(crate) async fn fetch_group(queries: &Queries, id: Uuid) -> Result<Groups, V2Error> {
    queries.get_group_by_id(id).await.map_err(|e| match e {
        DbError::NotFound => V2Error::not_found("Group not found."),
        other => V2Error::from(other),
    })
}
