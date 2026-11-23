use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::db::migrations::{MIGRATIONS, pending};

use super::StatusService;

impl StatusService {
    /// What a pod must have before it may receive traffic: an answering
    /// datastore, a schema with nothing left to apply, and the status pump
    /// owning the snapshot queue. Probe against this, not `/health`, which
    /// answers as soon as the process exists.
    pub async fn readyz(&self) -> Response {
        let db = self.plane.db.as_deref();
        let (database_ok, database) = match db {
            Some(db) => match crate::repositories::db::ping(db).await {
                Ok(()) => (true, "live".to_owned()),
                Err(err) => (false, format!("down: {err}")),
            },
            None if self.plane.control.session.database_endpoint.is_empty() => {
                (true, "not configured".to_owned())
            }
            None => (false, "down: the datastore never opened".to_owned()),
        };
        let (schema_ok, schema) = match db {
            Some(db) => match pending(db, MIGRATIONS).await {
                Ok(steps) if steps.is_empty() => (true, "migrated".to_owned()),
                Ok(steps) => (false, format!("{steps:?} pending")),
                Err(err) => (false, format!("unreadable: {err}")),
            },
            None => (database_ok, "not configured".to_owned()),
        };
        let pump_ok = self.plane.status_rx.is_taken();
        // A pod is not ready until the host has published a `Ready`/`Allocated`
        // lifecycle phase. No snapshot yet means the host is still booting.
        let (phase, lifecycle_ok) = match self
            .plane
            .latest_status
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
        {
            Some(snap) => (snap.server_phase, snap.server_phase.is_serving()),
            None => (engine_bus::ServerPhase::Booting, false),
        };
        let ready = database_ok && schema_ok && pump_ok && lifecycle_ok;
        let body = json!({
            "status": if ready { "ready" } else { "unready" },
            "checks": {
                "database": database,
                "schema": schema,
                "pump": if pump_ok { "running" } else { "not running" },
                "lifecycle": phase.as_str(),
            },
        });
        let status = if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        };
        (status, Json(body)).into_response()
    }
}
