use super::*;

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
