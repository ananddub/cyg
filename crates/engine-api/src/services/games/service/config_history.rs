use chrono::Utc;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{GamesService, UpdateGameParams, V2Error};
use crate::dto::console::config_history::ConfigVersionRow;
use crate::repositories::config_history::game_config_version_id;
use crate::repositories::console_mutations::InsertAuditLogParams;
use crate::repositories::models::GameConfigVersion;
use crate::services::deps::q;

use super::*;

fn config_sha(config: &Value) -> String {
    hex::encode(Sha256::digest(config.to_string().as_bytes()))
}

impl GamesService {
    /// Appends the history rows for a config change. Best-effort: a history
    /// failure is logged and never fails the update that produced it, and a
    /// body whose hash matches the newest stored version records nothing.
    pub async fn record_config_versions(
        state: &ApiState,
        game_id: Uuid,
        previous: &Value,
        current: &Value,
        author: Option<&str>,
    ) {
        if config_sha(previous) == config_sha(current) {
            return;
        }
        let Ok(queries) = q(state).await else {
            return;
        };
        let now = Utc::now();
        let latest = queries.latest_config_version(game_id).await.unwrap_or(0);
        let mut version = latest;
        if latest == 0 && config_sha(previous) != config_sha(&Value::Object(Default::default())) {
            version += 1;
            let seed = GameConfigVersion {
                id: game_config_version_id(game_id, version),
                game_id,
                version,
                sha256: config_sha(previous),
                is_active: false,
                body: previous.clone(),
                author: None,
                create_time: now,
            };
            if let Err(err) = queries.insert_config_version(&seed).await {
                tracing::warn!("config history: seeding the initial version failed: {err}");
                return;
            }
        }
        version += 1;
        let row = GameConfigVersion {
            id: game_config_version_id(game_id, version),
            game_id,
            version,
            sha256: config_sha(current),
            is_active: true,
            body: current.clone(),
            author: author.map(str::to_owned),
            create_time: now,
        };
        if let Err(err) = queries.insert_config_version(&row).await {
            tracing::warn!("config history: recording version {version} failed: {err}");
            return;
        }
        if let Err(err) = queries.activate_config_version(game_id, version).await {
            tracing::warn!("config history: retiring older versions failed: {err}");
        }
    }

    pub async fn config_history(
        state: &ApiState,
        game_id: Uuid,
    ) -> Result<Vec<ConfigVersionRow>, V2Error> {
        Self::find_game(state, game_id).await?;
        let rows = q(state).await?.list_config_versions(game_id).await?;
        Ok(rows.iter().map(|row| row.to_list_row()).collect())
    }

    pub async fn config_version(
        state: &ApiState,
        game_id: Uuid,
        version: i64,
    ) -> Result<ConfigVersionRow, V2Error> {
        q(state)
            .await?
            .get_config_version(game_id, version)
            .await?
            .map(|row| row.to_full_row())
            .ok_or_else(|| V2Error::not_found(format!("Config version {version} not found.")))
    }

    /// Rollback appends a new version carrying the old body — history is
    /// never rewritten — and swaps the live `games.config` in the same pass.
    pub async fn rollback_config(
        state: &ApiState,
        game_id: Uuid,
        version: i64,
        operator: Option<(Uuid, String, String)>,
    ) -> Result<ConfigVersionRow, V2Error> {
        let queries = q(state).await?;
        let target = queries
            .get_config_version(game_id, version)
            .await?
            .ok_or_else(|| V2Error::not_found(format!("Config version {version} not found.")))?;
        let current = Self::find_game(state, game_id).await?;

        let latest = queries.latest_config_version(game_id).await?;
        let new_version = latest + 1;
        let now = Utc::now();
        let author = operator.as_ref().map(|(_, username, _)| username.clone());
        let row = GameConfigVersion {
            id: game_config_version_id(game_id, new_version),
            game_id,
            version: new_version,
            sha256: config_sha(&target.body),
            is_active: true,
            body: target.body.clone(),
            author,
            create_time: now,
        };
        if !queries.insert_config_version(&row).await? {
            return Err(V2Error::already_exists(
                "That config version number is already taken.",
            ));
        }
        queries
            .activate_config_version(game_id, new_version)
            .await
            .map_err(V2Error::from)?;
        queries
            .update_game(&UpdateGameParams {
                id: game_id,
                name: current.name.clone(),
                engine_type: current.engine_type.clone(),
                status: current.status.clone(),
                config: target.body.clone(),
                backend_mode: current.backend_mode.clone(),
            })
            .await
            .map_err(V2Error::from)?;

        if let Some((user_id, username, email)) = operator {
            let _ = queries
                .insert_audit_log(&InsertAuditLogParams {
                    id: Uuid::new_v4(),
                    console_user_id: user_id,
                    console_username: username,
                    email,
                    action: "game.config.rollback".to_owned(),
                    resource: "game".to_owned(),
                    message: format!(
                        "Rolled back game {game_id} config to version {version} as {new_version}."
                    ),
                    metadata: serde_json::json!({ "version": version, "new_version": new_version }),
                })
                .await;
        }
        Ok(row.to_full_row())
    }
}
