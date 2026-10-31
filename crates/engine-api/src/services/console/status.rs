use axum::Json;

use crate::ApiState;
use crate::dto::console::status::{RouteRow, StatusResponse};
use crate::error::V2Error;

const METHODS: [&str; 7] = ["get", "post", "put", "patch", "delete", "head", "options"];

impl super::ConsoleService {
    pub async fn status(&self, state: &ApiState) -> Result<Json<StatusResponse>, V2Error> {
        let (accounts, console_users, database) = match super::queries(state).await {
            Ok(queries) => (
                queries.count_accounts().await.unwrap_or(0),
                queries.count_console_users().await.unwrap_or(0),
                true,
            ),
            Err(_) => (0, 0, false),
        };
        let control = &state.control;

        Ok(Json(StatusResponse {
            name: state.game.name.clone(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            api_port: state.port,
            uptime_secs: state.started.elapsed().as_secs() as i64,
            database,
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

        routes.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.method.cmp(&b.method)));
        Json(routes)
    }
}

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
