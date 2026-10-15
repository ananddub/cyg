//! `apiGroup`, `apiUserGroup` and `apiGroupUser` — the group shape, the two membership
//! edges that point a user at a group or a group at its members, and the requests that
//! create, update, list and administer them.

mod request;
mod response;

pub use request::{CreateGroupRequest, ListQuery, UpdateGroupRequest, UsersQuery};
pub use response::{
    GroupEnvelope, GroupListResponse, GroupResponse, GroupUserListResponse, GroupUserResponse,
    UserGroupListResponse, UserGroupResponse,
};
