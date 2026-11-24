use crate::services::plane::Plane;

use axum::Json;
use chrono::{DateTime, Utc};

use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::quest::{SeasonRequest, SeasonRow};
use crate::error::V2Error;
use crate::repositories::models::Season;
use crate::services::quest::catalog::definition_key;

impl super::ConsoleService {
    pub async fn list_seasons(&self) -> Result<Json<Vec<SeasonRow>>, V2Error> {
        let rows = self.plane.q().await?.list_seasons().await?;
        Ok(Json(rows.into_iter().map(SeasonRow::from).collect()))
    }

    pub async fn create_season(
        &self,

        claims: &ConsoleAuthed,
        body: SeasonRequest,
    ) -> Result<Json<SeasonRow>, V2Error> {
        claims.require_write()?;
        let key = definition_key(&body.season_key, "season")?;
        let queries = self.plane.q().await?;
        if queries.get_season(&key).await?.is_some() {
            return Err(V2Error::already_exists(
                "A season with this key already exists.",
            ));
        }
        let row = draft(&self.plane, key.clone(), body, Utc::now()).await?;
        queries.upsert_season(&row).await?;
        super::audit(
            &self.plane,
            claims,
            "season.create",
            "season",
            None,
            &format!("Created season {key}."),
        )
        .await;
        Ok(Json(SeasonRow::from(row)))
    }

    pub async fn update_season(
        &self,

        claims: &ConsoleAuthed,
        name: &str,
        body: SeasonRequest,
    ) -> Result<Json<SeasonRow>, V2Error> {
        claims.require_write()?;
        let key = definition_key(name, "season")?;
        let queries = self.plane.q().await?;
        let existing = queries.get_season(&key).await?;
        let row = draft(
            &self.plane,
            key.clone(),
            body,
            existing.map_or_else(Utc::now, |season| season.create_time),
        )
        .await?;
        queries.upsert_season(&row).await?;
        super::audit(
            &self.plane,
            claims,
            "season.update",
            "season",
            None,
            &format!("Wrote season {key}."),
        )
        .await;
        Ok(Json(SeasonRow::from(row)))
    }

    pub async fn delete_season(
        &self,

        claims: &ConsoleAuthed,
        name: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let key = definition_key(name, "season")?;
        let queries = self.plane.q().await?;
        if queries.delete_season(&key).await?.rows_affected() == 0 {
            return Err(V2Error::not_found("Season not found."));
        }
        // Quests naming this season stop running, which is the point of
        // deleting it, so they are left alone rather than rewritten.
        super::audit(
            &self.plane,
            claims,
            "season.delete",
            "season",
            None,
            &format!("Deleted season {key}."),
        )
        .await;
        Ok(Json(Empty {}))
    }
}

async fn draft(
    plane: &Plane,
    id: String,
    body: SeasonRequest,
    create_time: DateTime<Utc>,
) -> Result<Season, V2Error> {
    let title = body.title.trim();
    if title.is_empty() || title.len() > 120 {
        return Err(V2Error::invalid_argument(
            "A season needs a title of up to 120 characters.",
        ));
    }
    if body.starts_at >= body.ends_at {
        return Err(V2Error::invalid_argument(
            "A season has to end after it starts.",
        ));
    }
    Ok(Season {
        id,
        game_id: crate::services::games::tenant::game_id(plane).await?,
        title: title.to_string(),
        starts_at: body.starts_at,
        ends_at: body.ends_at,
        reward_track: body.reward_track,
        create_time,
        update_time: Utc::now(),
    })
}
