use auto_route::controller;
use axum::extract::Json;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use crate::app::middleware::Authed;
use crate::current_state;
use crate::dto::quest::ClaimQuestRequest;
use crate::services::quest::QuestService;

type JsonBody<T> = axum::extract::Json<T>;

struct QuestController {
    service: Arc<QuestService>,
}

#[controller("/v2/quest")]
impl QuestController {
    fn new(service: Arc<QuestService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, auth: Authed) -> Response {
        let state = current_state();
        self.service
            .list_quests(&state, &auth)
            .await
            .into_response()
    }

    #[post("/claim")]
    async fn claim(&self, auth: Authed, Json(body): JsonBody<ClaimQuestRequest>) -> Response {
        let state = current_state();
        self.service
            .claim_quest(&state, &auth, body)
            .await
            .into_response()
    }
}

struct DailyController {
    service: Arc<QuestService>,
}

#[controller("/v2/account/daily")]
impl DailyController {
    fn new(service: Arc<QuestService>) -> Self {
        Self { service }
    }

    #[post("/claim")]
    async fn claim(&self, auth: Authed) -> Response {
        let state = current_state();
        self.service
            .claim_daily(&state, &auth)
            .await
            .into_response()
    }
}
