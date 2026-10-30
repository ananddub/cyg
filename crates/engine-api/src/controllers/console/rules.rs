use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Body;
use crate::current_state;
use crate::dto::console::rules::{RuleDryRun, RuleRequest};
use crate::error::V2Error;
use crate::services::console::ConsoleService;

struct ConsoleRulesController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/rules")]
impl ConsoleRulesController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_rules(&state).await.into_response()
    }

    #[post("")]
    async fn create(&self, auth: ConsoleAuthed, Body(body): Body<RuleRequest>) -> Response {
        let state = current_state();
        self.service
            .create_rule(&state, &auth, body)
            .await
            .into_response()
    }

    #[get("/{key}")]
    async fn versions(&self, _auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service
            .rule_versions(&state, &key)
            .await
            .into_response()
    }

    #[post("/{key}/publish/{version}")]
    async fn publish(
        &self,
        auth: ConsoleAuthed,
        Path((key, version)): Path<(String, String)>,
    ) -> Response {
        let state = current_state();
        let Ok(version) = version.parse::<i64>() else {
            return V2Error::invalid_argument("A rule version is a number.").into_response();
        };
        self.service
            .publish_rule(&state, &auth, &key, version)
            .await
            .into_response()
    }

    #[delete("/{key}")]
    async fn delete(&self, auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_rule(&state, &auth, &key)
            .await
            .into_response()
    }

    #[post("/dry-run")]
    async fn dry_run(&self, _auth: ConsoleAuthed, Body(body): Body<RuleDryRun>) -> Response {
        let state = current_state();
        self.service
            .dry_run_rule(&state, body)
            .await
            .into_response()
    }
}
