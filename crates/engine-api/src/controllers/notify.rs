use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::{Authed, ConsoleAuthed};
use crate::app::params::{Body, Params};
use crate::current_state;
use crate::dto::notify::{PreferenceRequest, TestRequest, UnsubscribeQuery};
use crate::services::notify::NotifyService;

/// The switches an account holds over what leaves the app for it, the link a
/// recipient clicks instead of finding those switches, and the one probe an
/// operator runs to ask whether any of it is actually reachable.
struct NotifyController {
    service: Arc<NotifyService>,
}

#[controller("/v2/notify")]
impl NotifyController {
    fn new(service: Arc<NotifyService>) -> Self {
        Self { service }
    }

    #[get("/preferences")]
    async fn list(&self, auth: Authed) -> Response {
        let state = current_state();
        self.service.list(&state, &auth).await.into_response()
    }

    #[put("/preferences")]
    async fn set(&self, auth: Authed, Body(body): Body<PreferenceRequest>) -> Response {
        let state = current_state();
        self.service.set(&state, &auth, body).await.into_response()
    }

    /// Unauthenticated on purpose: this is the link in a mail, opened in a mail
    /// client that has no session here. The proof is the signature on the token.
    #[get("/unsubscribe")]
    async fn unsubscribe(&self, Params(query): Params<UnsubscribeQuery>) -> Response {
        let state = current_state();
        self.service
            .unsubscribe(&state, &query.token)
            .await
            .into_response()
    }

    #[post("/test")]
    async fn test(&self, auth: ConsoleAuthed, Body(body): Body<TestRequest>) -> Response {
        let state = current_state();
        self.service.test(&state, &auth, body).await.into_response()
    }

    /// What became of one message, so a queued test is not the end of the
    /// operator's question.
    #[get("/test/{attempt_id}")]
    async fn attempt(&self, auth: ConsoleAuthed, Path(attempt_id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .attempt(&state, &auth, &attempt_id)
            .await
            .into_response()
    }
}
