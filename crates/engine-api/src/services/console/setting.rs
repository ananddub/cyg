//! `/v2/console/setting` — the server's key/value settings.

use axum::Json;

use crate::dto::console::setting::SettingRow;
use crate::error::V2Error;
use crate::ApiState;

impl super::ConsoleService {
    pub async fn list_settings(
        &self,
        state: &ApiState,
    ) -> Result<Json<Vec<SettingRow>>, V2Error> {
        let rows = super::queries(state)?.list_settings().await?;
        Ok(Json(rows.into_iter().map(SettingRow::from).collect()))
    }
}
