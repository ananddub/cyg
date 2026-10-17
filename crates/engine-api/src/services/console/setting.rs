//! `/v2/console/setting` — the server's key/value settings, read and written.

use axum::Json;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::setting::{SettingRow, UpdateSettingRequest};
use crate::error::V2Error;
use crate::ApiState;

impl super::ConsoleService {
    pub async fn list_settings(&self, state: &ApiState) -> Result<Json<Vec<SettingRow>>, V2Error> {
        let rows = super::queries(state)?.list_settings().await?;
        Ok(Json(rows.into_iter().map(SettingRow::from).collect()))
    }

    /// Write a setting. A setting is its own key, so this creates or replaces and
    /// neither outcome is an error.
    pub async fn update_setting(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        body: UpdateSettingRequest,
    ) -> Result<Json<SettingRow>, V2Error> {
        claims.require_admin()?;
        let name = body.name.trim();
        if name.is_empty() {
            return Err(V2Error::invalid_argument("A setting needs a name."));
        }

        let queries = super::queries(state)?;
        queries.upsert_setting(name, body.value).await?;
        super::audit(
            state,
            claims,
            "setting.update",
            "setting",
            None,
            &format!("Wrote setting {name}."),
        )
        .await;

        // Read back so the reply carries the row's own `update_time` rather than one
        // this handler guessed at.
        let settings = queries.list_settings().await?;
        settings
            .into_iter()
            .find(|setting| setting.name == name)
            .map(|setting| Json(SettingRow::from(setting)))
            .ok_or_else(|| V2Error::internal("The setting was written but could not be read back."))
    }

    pub async fn delete_setting(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        name: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_admin()?;
        let name = name.trim();
        if name.is_empty() {
            return Err(V2Error::invalid_argument("A setting needs a name."));
        }
        let queries = super::queries(state)?;
        if queries.delete_setting(name).await?.rows_affected() == 0 {
            return Err(V2Error::not_found("Setting not found."));
        }
        super::audit(
            state,
            claims,
            "setting.delete",
            "setting",
            None,
            &format!("Deleted setting {name}."),
        )
        .await;
        Ok(Json(Empty {}))
    }
}
