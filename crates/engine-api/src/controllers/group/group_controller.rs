use super::*;

struct GroupController {
    groups: Arc<GroupService>,
    members: Arc<GroupMemberService>,
}
#[controller("/v1/group")]
impl GroupController {
    fn new(groups: Arc<GroupService>, members: Arc<GroupMemberService>) -> Self {
        Self { groups, members }
    }

    #[get("", response_body = crate::dto::group::GroupListResponse)]
    async fn list_groups(&self, auth: Authed, Params(req): Params<ListQuery>) -> Response {
        let state = current_state();
        self.groups.list_groups(&auth, req).await.into_response()
    }

    #[post("", response_body = crate::dto::group::GroupEnvelope)]
    async fn create_group(&self, auth: Authed, Body(body): Body<CreateGroupRequest>) -> Response {
        let state = current_state();
        self.groups.create_group(&auth, body).await.into_response()
    }

    #[get("/{group_id}", response_body = crate::dto::group::GroupEnvelope)]
    async fn get_group(&self, auth: Authed, Path(raw_id): Path<String>) -> Response {
        let state = current_state();
        self.groups.get_group(&auth, raw_id).await.into_response()
    }

    #[put("/{group_id}", response_body = crate::dto::Empty)]
    async fn update_group(
        &self,
        auth: Authed,
        Path(raw_id): Path<String>,
        Body(body): Body<UpdateGroupRequest>,
    ) -> Response {
        let state = current_state();
        self.groups
            .update_group(&auth, raw_id, body)
            .await
            .into_response()
    }

    #[delete("/{group_id}", response_body = crate::dto::Empty)]
    async fn delete_group(&self, auth: Authed, Path(raw_id): Path<String>) -> Response {
        let state = current_state();
        self.groups
            .delete_group(&auth, raw_id)
            .await
            .into_response()
    }

    #[post("/{group_id}/join", response_body = crate::dto::Empty)]
    async fn join_group(&self, auth: Authed, Path(raw_id): Path<String>) -> Response {
        let state = current_state();
        self.groups.join_group(&auth, raw_id).await.into_response()
    }

    #[post("/{group_id}/leave", response_body = crate::dto::Empty)]
    async fn leave_group(&self, auth: Authed, Path(raw_id): Path<String>) -> Response {
        let state = current_state();
        self.groups.leave_group(&auth, raw_id).await.into_response()
    }

    #[get("/{group_id}/user", response_body = crate::dto::group::GroupUserListResponse)]
    async fn list_group_users(
        &self,
        auth: Authed,
        Path(raw_id): Path<String>,
        Params(req): Params<ListQuery>,
    ) -> Response {
        let state = current_state();
        self.members
            .list_group_users(&auth, raw_id, req)
            .await
            .into_response()
    }

    #[post("/{group_id}/add", response_body = crate::dto::Empty)]
    async fn add_group_users(
        &self,
        auth: Authed,
        Path(raw_id): Path<String>,
        Params(req): Params<UsersQuery>,
    ) -> Response {
        let state = current_state();
        self.members
            .add_group_users(&auth, raw_id, req)
            .await
            .into_response()
    }

    #[post("/{group_id}/kick", response_body = crate::dto::Empty)]
    async fn kick_group_users(
        &self,
        auth: Authed,
        Path(raw_id): Path<String>,
        Params(req): Params<UsersQuery>,
    ) -> Response {
        let state = current_state();
        self.members
            .kick_group_users(&auth, raw_id, req)
            .await
            .into_response()
    }

    #[post("/{group_id}/ban", response_body = crate::dto::Empty)]
    async fn ban_group_users(
        &self,
        auth: Authed,
        Path(raw_id): Path<String>,
        Params(req): Params<UsersQuery>,
    ) -> Response {
        let state = current_state();
        self.members
            .ban_group_users(&auth, raw_id, req)
            .await
            .into_response()
    }

    #[post("/{group_id}/promote", response_body = crate::dto::Empty)]
    async fn promote_group_users(
        &self,
        auth: Authed,
        Path(raw_id): Path<String>,
        Params(req): Params<UsersQuery>,
    ) -> Response {
        let state = current_state();
        self.members
            .promote_group_users(&auth, raw_id, req)
            .await
            .into_response()
    }

    #[post("/{group_id}/demote", response_body = crate::dto::Empty)]
    async fn demote_group_users(
        &self,
        auth: Authed,
        Path(raw_id): Path<String>,
        Params(req): Params<UsersQuery>,
    ) -> Response {
        let state = current_state();
        self.members
            .demote_group_users(&auth, raw_id, req)
            .await
            .into_response()
    }
}
