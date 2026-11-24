use super::*;

struct UserGroupController {
    service: Arc<UserGroupService>,
}
#[controller("/v1/user")]
impl UserGroupController {
    fn new(service: Arc<UserGroupService>) -> Self {
        Self { service }
    }

    #[get("/{user_id}/group", response_body = crate::dto::group::UserGroupListResponse)]
    async fn list_user_groups(
        &self,
        auth: Authed,
        Path(raw_user): Path<String>,
        Params(req): Params<ListQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_user_groups(&auth, raw_user, req)
            .await
            .into_response()
    }
}
