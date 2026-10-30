use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::ApiState;
use crate::db::migrations::{MIGRATIONS, pending};

use super::StatusService;

impl StatusService {
    /// What a pod must have before it may receive traffic: an answering
    /// datastore, a schema with nothing left to apply, and the status pump
    /// owning the snapshot queue. Probe against this, not `/health`, which
    /// answers as soon as the process exists.
    pub async fn readyz(&self, state: &ApiState) -> Response {
        let db = state.db.as_deref();
        let (database_ok, database) = match db {
            Some(db) => match db.exec("RETURN 1;").await {
                Ok(()) => (true, "live".to_owned()),
                Err(err) => (false, format!("down: {err}")),
            },
            None if state.control.session.database_endpoint.is_empty() => {
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
        let pump_ok = state.status_rx.is_taken();
        let ready = database_ok && schema_ok && pump_ok;
        let body = json!({
            "status": if ready { "ready" } else { "unready" },
            "checks": {
                "database": database,
                "schema": schema,
                "pump": if pump_ok { "running" } else { "not running" },
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
