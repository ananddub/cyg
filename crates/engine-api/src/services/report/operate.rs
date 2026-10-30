use super::*;
use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::report::{
    ListQuery, ReportListResponse, ReportResponse, STATUSES, StatusRequest,
};
use crate::error::V2Error;
use crate::repositories::report::{
    CreateInfractionParams, ListReportsByStatusPagedParams, ListReportsPagedParams,
};
use crate::services::deps::q;
use crate::services::social::{cursor_decode, limit, ts_head};
use crate::utils::notification::{LIMIT_RANGE, encode_cursor_at};

const MAX_MUTE_SECONDS: i64 = 30 * 24 * 60 * 60;

impl ReportService {
    pub async fn list(
        &self,
        state: &ApiState,
        auth: &ConsoleAuthed,
        req: ListQuery,
    ) -> Result<Json<ReportListResponse>, V2Error> {
        auth.require_write()?;
        let limit_n = limit(req.limit, 50, 100, LIMIT_RANGE)?;
        if let Some(status) = req.status.as_deref() {
            if !STATUSES.contains(&status) {
                return Err(V2Error::invalid_argument("Invalid report status."));
            }
        }
        let cursor = decode(req.cacheable_cursor.as_deref())?;
        let (create_time, id_start) = cursor.unwrap_or((ts_head(), Uuid::nil()));
        let queries = q(state).await?;
        let mut rows = match req.status.as_deref() {
            Some(status) => {
                queries
                    .list_reports_by_status_paged(&ListReportsByStatusPagedParams {
                        status: status.to_owned(),
                        create_time,
                        id_start,
                        limit: limit_n + 1,
                    })
                    .await?
            }
            None => {
                queries
                    .list_reports_paged(&ListReportsPagedParams {
                        create_time,
                        id_start,
                        limit: limit_n + 1,
                    })
                    .await?
            }
        };
        if rows.len() as i64 > limit_n {
            rows.pop();
        }
        let reports = rows.iter().map(ReportResponse::from).collect();
        let cacheable_cursor = match rows.last() {
            Some(last) => encode_cursor_at(last.create_time, last.id),
            None => req
                .cacheable_cursor
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| encode_cursor_at(ts_head(), Uuid::nil())),
        };
        Ok(Json(ReportListResponse {
            reports,
            cacheable_cursor,
        }))
    }

    pub async fn set_status(
        &self,
        state: &ApiState,
        auth: &ConsoleAuthed,
        id: &str,
        req: StatusRequest,
    ) -> Result<Json<ReportResponse>, V2Error> {
        auth.require_write()?;
        let id =
            Uuid::parse_str(id.trim()).map_err(|_| V2Error::invalid_argument("Invalid report ID."))?;
        if !STATUSES.contains(&req.status.as_str()) {
            return Err(V2Error::invalid_argument("Invalid report status."));
        }
        let queries = q(state).await?;
        let report = queries
            .update_report_status(id, &req.status)
            .await?
            .ok_or_else(|| V2Error::not_found("Report not found."))?;
        if req.status == "actioned" {
            record_infraction(&queries, auth, &report.reported_id, &req).await?;
        }
        Ok(Json(ReportResponse::from(&report)))
    }
}

async fn record_infraction(
    queries: &crate::repositories::db::Queries,
    auth: &ConsoleAuthed,
    reported_id: &Uuid,
    req: &StatusRequest,
) -> Result<(), V2Error> {
    let kind = match req.kind.as_deref() {
        Some(kind @ ("warn" | "mute")) => kind.to_owned(),
        Some(_) => return Err(V2Error::invalid_argument("Invalid infraction kind.")),
        None => return Ok(()),
    };
    let duration = req.duration_secs.unwrap_or(0);
    if !(1..=MAX_MUTE_SECONDS).contains(&duration) {
        return Err(V2Error::invalid_argument(
            "Infraction duration must be between 1 and 2592000 seconds.",
        ));
    }
    queries.create_infraction(&CreateInfractionParams {
        id: Uuid::new_v4(),
        user_id: *reported_id,
        kind,
        note: crate::moderation::clean_text(&req.note),
        expires_at: Utc::now() + chrono::Duration::seconds(duration),
        issued_by: auth.0.uuid(),
    })
    .await?;
    Ok(())
}

fn decode(raw: Option<&str>) -> Result<Option<(chrono::DateTime<Utc>, Uuid)>, V2Error> {
    let Some(raw) = raw.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let v = cursor_decode(raw, LIMIT_CURSOR)?;
    let time = v
        .get("create_time")
        .and_then(serde_json::Value::as_str)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| V2Error::invalid_argument(LIMIT_CURSOR))?;
    let id = v
        .get("id")
        .and_then(serde_json::Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or(Uuid::nil());
    Ok(Some((time, id)))
}

const LIMIT_CURSOR: &str = "Invalid cacheable cursor.";
