//! Auto-registered bindings for `/v2/console/mfa` — the signed-in operator's own
//! second factor.

use std::sync::Arc;

use auto_route::controller;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Body;
use crate::current_state;
use crate::dto::console::auth::MfaCodeRequest;
use crate::services::console::ConsoleService;

struct ConsoleMfaController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/mfa")]
impl ConsoleMfaController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[post("")]
    async fn enrol(&self, auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.enrol_mfa(&state, &auth).await.into_response()
    }

    /// Confirmation carries the code in the body rather than the query string, so a
    /// digit that is briefly current does not end up in an access log.
    #[post("/verify")]
    async fn confirm(&self, auth: ConsoleAuthed, Body(body): Body<MfaCodeRequest>) -> Response {
        let state = current_state();
        self.service
            .confirm_mfa(&state, &auth, body)
            .await
            .into_response()
    }

    #[delete("")]
    async fn disable(&self, auth: ConsoleAuthed, Body(body): Body<MfaCodeRequest>) -> Response {
        let state = current_state();
        self.service
            .disable_mfa(&state, &auth, body)
            .await
            .into_response()
    }
}
