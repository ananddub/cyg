//! Auto-registered bindings for `/v2/group*`, plus the `/v2/user/{user_id}/group`
//! alias. Transport only: name the bindings, take the extractors, call the service.

use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::Authed;
use crate::app::params::{Body, Params};
use crate::current_state;

use crate::dto::group::{CreateGroupRequest, ListQuery, UpdateGroupRequest, UsersQuery};
use crate::services::group::{GroupMemberService, GroupService, UserGroupService};

struct GroupController {
    groups: Arc<GroupService>,
    members: Arc<GroupMemberService>,
}

#[controller("/v2/group")]
impl GroupController {
    fn new(groups: Arc<GroupService>, members: Arc<GroupMemberService>) -> Self {
        Self { groups, members }
    }

    #[get("")]
    async fn list_groups(&self, auth: Authed, Params(req): Params<ListQuery>) -> Response {
        let state = current_state();
        self.groups
            .list_groups(&state, &auth, req)
            .await
            .into_response()
    }

    #[post("")]
    async fn create_group(&self, auth: Authed, Body(body): Body<CreateGroupRequest>) -> Response {
        let state = current_state();
        self.groups
            .create_group(&state, &auth, body)
            .await
            .into_response()
    }

    #[get("/{group_id}")]
    async fn get_group(&self, auth: Authed, Path(raw_id): Path<String>) -> Response {
        let state = current_state();
        self.groups
            .get_group(&state, &auth, raw_id)
            .await
            .into_response()
    }

    #[put("/{group_id}")]
    async fn update_group(
        &self,
        auth: Authed,
        Path(raw_id): Path<String>,
        Body(body): Body<UpdateGroupRequest>,
    ) -> Response {
        let state = current_state();
        self.groups
            .update_group(&state, &auth, raw_id, body)
            .await
            .into_response()
    }

    #[delete("/{group_id}")]
    async fn delete_group(&self, auth: Authed, Path(raw_id): Path<String>) -> Response {
        let state = current_state();
        self.groups
            .delete_group(&state, &auth, raw_id)
            .await
            .into_response()
    }

    #[post("/{group_id}/join")]
    async fn join_group(&self, auth: Authed, Path(raw_id): Path<String>) -> Response {
        let state = current_state();
        self.groups
            .join_group(&state, &auth, raw_id)
            .await
            .into_response()
    }

    #[post("/{group_id}/leave")]
    async fn leave_group(&self, auth: Authed, Path(raw_id): Path<String>) -> Response {
        let state = current_state();
        self.groups
            .leave_group(&state, &auth, raw_id)
            .await
            .into_response()
    }

    #[get("/{group_id}/user")]
    async fn list_group_users(
        &self,
        auth: Authed,
        Path(raw_id): Path<String>,
        Params(req): Params<ListQuery>,
    ) -> Response {
        let state = current_state();
        self.members
            .list_group_users(&state, &auth, raw_id, req)
            .await
            .into_response()
    }

    #[post("/{group_id}/add")]
    async fn add_group_users(
        &self,
        auth: Authed,
        Path(raw_id): Path<String>,
        Params(req): Params<UsersQuery>,
    ) -> Response {
        let state = current_state();
        self.members
            .add_group_users(&state, &auth, raw_id, req)
            .await
            .into_response()
    }

    #[post("/{group_id}/kick")]
    async fn kick_group_users(
        &self,
        auth: Authed,
        Path(raw_id): Path<String>,
        Params(req): Params<UsersQuery>,
    ) -> Response {
        let state = current_state();
        self.members
            .kick_group_users(&state, &auth, raw_id, req)
            .await
            .into_response()
    }

    #[post("/{group_id}/ban")]
    async fn ban_group_users(
        &self,
        auth: Authed,
        Path(raw_id): Path<String>,
        Params(req): Params<UsersQuery>,
    ) -> Response {
        let state = current_state();
        self.members
            .ban_group_users(&state, &auth, raw_id, req)
            .await
            .into_response()
    }

    #[post("/{group_id}/promote")]
    async fn promote_group_users(
        &self,
        auth: Authed,
        Path(raw_id): Path<String>,
        Params(req): Params<UsersQuery>,
    ) -> Response {
        let state = current_state();
        self.members
            .promote_group_users(&state, &auth, raw_id, req)
            .await
            .into_response()
    }

    #[post("/{group_id}/demote")]
    async fn demote_group_users(
        &self,
        auth: Authed,
        Path(raw_id): Path<String>,
        Params(req): Params<UsersQuery>,
    ) -> Response {
        let state = current_state();
        self.members
            .demote_group_users(&state, &auth, raw_id, req)
            .await
            .into_response()
    }
}

/// The group surface also owns `/v2/user/{user_id}/group`, which shares the
/// `/v2/user` prefix with `UserController` in `user.rs`.
struct UserGroupController {
    service: Arc<UserGroupService>,
}

#[controller("/v2/user")]
impl UserGroupController {
    fn new(service: Arc<UserGroupService>) -> Self {
        Self { service }
    }

    #[get("/{user_id}/group")]
    async fn list_user_groups(
        &self,
        auth: Authed,
        Path(raw_user): Path<String>,
        Params(req): Params<ListQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_user_groups(&state, &auth, raw_user, req)
            .await
            .into_response()
    }
}
