//! Auto-registered bindings for `/v2/console/purchase` and
//! `/v2/console/subscription`.

use std::sync::Arc;

use auto_route::controller;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Params;
use crate::current_state;
use crate::dto::console::Page;
use crate::services::console::ConsoleService;

struct ConsolePurchaseController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/purchase")]
impl ConsolePurchaseController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service.list_purchases(&state, page).await.into_response()
    }
}

struct ConsoleSubscriptionController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/subscription")]
impl ConsoleSubscriptionController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service
            .list_subscriptions(&state, page)
            .await
            .into_response()
    }
}
