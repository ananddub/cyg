//! `/v2/console/user` — the console operators themselves.

use axum::Json;

use crate::dto::console::Page;
use crate::dto::console::user::ConsoleUserView;
use crate::error::V2Error;
use crate::ApiState;

impl super::ConsoleService {
    pub async fn list_console_users(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<Vec<ConsoleUserView>>, V2Error> {
        let rows = super::queries(state)?
            .list_console_users(page.limit(), page.offset())
            .await?;
        Ok(Json(rows.into_iter().map(ConsoleUserView::from).collect()))
    }
}
