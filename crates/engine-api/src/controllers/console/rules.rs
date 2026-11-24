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

type RuleRowList = Vec<crate::dto::console::rules::RuleRow>;
#[controller("/v1/console/rules")]
impl ConsoleRulesController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("", response_body = RuleRowList)]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_rules().await.into_response()
    }

    #[post("", response_body = crate::dto::console::rules::RuleRow)]
    async fn create(&self, auth: ConsoleAuthed, Body(body): Body<RuleRequest>) -> Response {
        let state = current_state();
        self.service.create_rule(&auth, body).await.into_response()
    }

    #[get("/{key}", response_body = RuleRowList)]
    async fn versions(&self, _auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service.rule_versions(&key).await.into_response()
    }

    #[post("/{key}/publish/{version}", response_body = crate::dto::console::rules::RuleRow)]
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
            .publish_rule(&auth, &key, version)
            .await
            .into_response()
    }

    #[delete("/{key}", response_body = crate::dto::console::rules::DeletedRuleResponse)]
    async fn delete(&self, auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service.delete_rule(&auth, &key).await.into_response()
    }

    #[post("/dry-run", response_body = crate::dto::console::rules::RuleDryRunResult)]
    async fn dry_run(&self, _auth: ConsoleAuthed, Body(body): Body<RuleDryRun>) -> Response {
        let state = current_state();
        self.service.dry_run_rule(body).await.into_response()
    }
}
