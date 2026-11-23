use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::{Body, Params};
use crate::current_state;
use crate::dto::console::iap::ConsoleValidateRequest;
use crate::dto::console::{GameScope, Page};
use crate::services::console::ConsoleService;

struct ConsolePurchaseController {
    service: Arc<ConsoleService>,
}

type PurchaseRowList = Vec<crate::dto::console::iap::PurchaseRow>;
type SubscriptionRowList = Vec<crate::dto::console::iap::SubscriptionRow>;
#[controller("/v1/console/purchase")]
impl ConsolePurchaseController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("", response_body = PurchaseRowList)]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service.list_purchases(page).await.into_response()
    }
}

struct ConsoleSubscriptionController {
    service: Arc<ConsoleService>,
}

#[controller("/v1/console/subscription")]
impl ConsoleSubscriptionController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("", response_body = SubscriptionRowList)]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service.list_subscriptions(page).await.into_response()
    }

    #[delete("/{product_id}", response_body = crate::dto::Empty)]
    async fn delete_by_product(
        &self,
        auth: ConsoleAuthed,
        Path(product_id): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        self.service
            .delete_subscription_product(&auth, scope.id(), &product_id)
            .await
            .into_response()
    }
}

struct ConsoleIapController {
    service: Arc<ConsoleService>,
}

#[controller("/v1/console/iap")]
impl ConsoleIapController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[post("/{vendor}/validate", response_body = serde_json::Value)]
    async fn validate(
        &self,
        auth: ConsoleAuthed,
        Path(vendor): Path<String>,
        Body(body): Body<ConsoleValidateRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .validate_vendor(&auth, &vendor, body)
            .await
            .into_response()
    }

    #[delete("/{vendor}/reset", response_body = crate::dto::Empty)]
    async fn reset(&self, auth: ConsoleAuthed, Path(vendor): Path<String>) -> Response {
        let state = current_state();
        self.service
            .reset_vendor(&auth, &vendor)
            .await
            .into_response()
    }
}
