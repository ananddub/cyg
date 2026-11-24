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

#[controller("/v1/quest")]
impl QuestController {
    fn new(service: Arc<QuestService>) -> Self {
        Self { service }
    }

    #[get("", response_body = crate::dto::quest::QuestListResponse)]
    async fn list(&self, auth: Authed) -> Response {
        let state = current_state();
        self.service.list_quests(&auth).await.into_response()
    }

    #[post("/claim", response_body = crate::dto::quest::QuestClaimResponse)]
    async fn claim(&self, auth: Authed, Json(body): JsonBody<ClaimQuestRequest>) -> Response {
        let state = current_state();
        self.service.claim_quest(&auth, body).await.into_response()
    }
}

struct DailyController {
    service: Arc<QuestService>,
}

#[controller("/v1/account/daily")]
impl DailyController {
    fn new(service: Arc<QuestService>) -> Self {
        Self { service }
    }

    #[post("/claim", response_body = crate::dto::quest::DailyClaimResponse)]
    async fn claim(&self, auth: Authed) -> Response {
        let state = current_state();
        self.service.claim_daily(&auth).await.into_response()
    }
}
