use axum::Json;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::setting::{SettingRow, UpdateSettingRequest};
use crate::error::V2Error;

impl super::ConsoleService {
    pub async fn list_settings(&self) -> Result<Json<Vec<SettingRow>>, V2Error> {
        let rows = self.plane.q().await?.list_settings().await?;
        Ok(Json(rows.into_iter().map(SettingRow::from).collect()))
    }

    pub async fn update_setting(
        &self,

        claims: &ConsoleAuthed,
        body: UpdateSettingRequest,
    ) -> Result<Json<SettingRow>, V2Error> {
        claims.require_admin()?;
        let name = body.name.trim();
        if name.is_empty() {
            return Err(V2Error::invalid_argument("A setting needs a name."));
        }

        let queries = self.plane.q().await?;
        queries.upsert_setting(name, body.value).await?;
        super::audit(
            &self.plane,
            claims,
            "setting.update",
            "setting",
            None,
            &format!("Wrote setting {name}."),
        )
        .await;

        let settings = queries.list_settings().await?;
        settings
            .into_iter()
            .find(|setting| setting.name == name)
            .map(|setting| Json(SettingRow::from(setting)))
            .ok_or_else(|| V2Error::internal("The setting was written but could not be read back."))
    }

    pub async fn delete_setting(
        &self,

        claims: &ConsoleAuthed,
        name: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_admin()?;
        let name = name.trim();
        if name.is_empty() {
            return Err(V2Error::invalid_argument("A setting needs a name."));
        }
        let queries = self.plane.q().await?;
        if queries.delete_setting(name).await?.rows_affected() == 0 {
            return Err(V2Error::not_found("Setting not found."));
        }
        super::audit(
            &self.plane,
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
