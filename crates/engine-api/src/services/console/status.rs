//! `/v2/console/status` and `/v2/console/route` — the console's own view of the
//! server it is attached to, and of the routes it can call.
//!
//! What is reported is deliberately not configuration: ports, worker counts and
//! lifetimes are facts an operator can act on, while keys and DSNs are the ones a
//! status page must never print.

use axum::Json;

use crate::ApiState;
use crate::dto::console::status::{RouteRow, StatusResponse};
use crate::error::V2Error;

/// The methods an OpenAPI path object can carry, in a fixed order so the explorer's
/// list does not reshuffle between calls.
const METHODS: [&str; 7] = ["get", "post", "put", "patch", "delete", "head", "options"];

impl super::ConsoleService {
    pub async fn status(&self, state: &ApiState) -> Result<Json<StatusResponse>, V2Error> {
        // The counts need a database; the rest of the page does not, so a missing
        // pool reports zero rather than failing the whole status.
        let (accounts, console_users) = match state.q.clone() {
            Some(queries) => (
                queries.count_accounts().await.unwrap_or(0),
                queries.count_console_users().await.unwrap_or(0),
            ),
            None => (0, 0),
        };
        let control = &state.control;

        Ok(Json(StatusResponse {
            name: state.game.name.clone(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            api_port: state.port,
            uptime_secs: state.started.elapsed().as_secs() as i64,
            database: state.q.is_some(),
            accounts,
            console_users,
            workers: control.workers,
            seat_ttl_secs: control.seat_ttl_secs,
            rate_limit_rps: control.rate_limit_rps,
            rate_limit_burst: control.rate_limit_burst,
            player_token_expiry_secs: control.session.token_expiry_secs,
            console_token_expiry_secs: control.console.token_expiry_secs,
            agones: control.agones.enabled,
        }))
    }

    /// The routes the API explorer offers, read from the same OpenAPI document the
    /// route macros register — so the explorer cannot drift from the routes
    /// themselves, and a new controller appears in it without anything to update.
    pub async fn routes(&self) -> Json<Vec<RouteRow>> {
        let spec = auto_route::openapi_json();
        let mut routes = Vec::new();
        if let Some(paths) = spec.get("paths").and_then(serde_json::Value::as_object) {
            for (path, item) in paths {
                let Some(operations) = item.as_object() else {
                    continue;
                };
                for method in METHODS {
                    let Some(operation) = operations.get(method) else {
                        continue;
                    };
                    routes.push(RouteRow {
                        method: method.to_uppercase(),
                        path: path.clone(),
                        tags: string_list(operation.get("tags")),
                        summary: operation
                            .get("summary")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                    });
                }
            }
        }

        // A stable order: the spec's map order is not a contract, and an explorer
        // whose list reshuffles is one nobody can use.
        routes.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.method.cmp(&b.method)));
        Json(routes)
    }
}

/// A JSON array of strings, ignoring anything that is not one.
fn string_list(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
