use super::*;
use axum::Json;
use uuid::Uuid;

use crate::app::middleware::Authed;
use crate::dto::report::{CreateReportRequest, REASONS, ReportResponse};
use crate::error::V2Error;
use crate::moderation;
use crate::repositories::report::CreateReportParams;
use crate::services::deps::q;

const MAX_DETAILS: usize = 1000;
const MAX_PATH: usize = 512;

impl ReportService {
    pub async fn create(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: CreateReportRequest,
    ) -> Result<Json<ReportResponse>, V2Error> {
        let reported_id = Uuid::parse_str(req.reported_id.trim())
            .map_err(|_| V2Error::invalid_argument("Invalid reported user ID."))?;
        let reporter_id = auth.uid();
        if reported_id == reporter_id {
            return Err(V2Error::invalid_argument("Cannot report your own account."));
        }
        if !REASONS.contains(&req.reason.as_str()) {
            return Err(V2Error::invalid_argument(
                "Invalid report reason: must be harassment, cheating, exploit, inappropriate_content or other.",
            ));
        }
        let details = clip(moderation::clean_text(&req.details), MAX_DETAILS);
        let path = req
            .path
            .as_deref()
            .map(|raw| clip(moderation::clean_text(raw), MAX_PATH));
        let queries = q(state).await?;
        queries
            .get_user_by_id(reported_id)
            .await
            .map_err(|_| V2Error::not_found("Reported user not found."))?;
        let report = queries
            .create_report(&CreateReportParams {
                id: Uuid::new_v4(),
                reporter_id,
                reported_id,
                reason: req.reason,
                details,
                path,
            })
            .await?;
        Ok(Json(ReportResponse::from(&report)))
    }
}

fn clip(mut text: String, max: usize) -> String {
    if text.chars().count() > max {
        text = text.chars().take(max).collect();
    }
    text
}
