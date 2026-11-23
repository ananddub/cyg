use std::sync::Arc;

use auto_route::controller;
use axum::extract::{Json, Path};
use axum::response::{IntoResponse, Response};

use crate::ApiError;
use crate::current_state;
use crate::services::queue::{QueueJoin, QueueService};

type JsonBody<T> = axum::extract::Json<T>;

struct QueueController {
    service: Arc<QueueService>,
}

async fn join_inner(service: &QueueService, req: QueueJoin) -> Result<Response, ApiError> {
    let state = current_state();
    let view = service.join(req)?;
    if view.status == "matched" {
        // Minting is idempotent and guarded by the queue lock.
        service.seats(&view.ticket_id)?;
    }
    Ok(axum::Json(service.status(&view.ticket_id)?).into_response())
}

async fn status_inner(service: &QueueService, ticket: String) -> Result<Response, ApiError> {
    let state = current_state();
    let view = service.status(&ticket)?;
    if view.status == "matched" {
        service.seats(&ticket)?;
    }
    Ok(axum::Json(service.status(&ticket)?).into_response())
}

async fn leave_inner(service: &QueueService, ticket: String) -> Result<Response, ApiError> {
    Ok(axum::Json(service.leave(&ticket)?).into_response())
}

#[controller("/queue")]
impl QueueController {
    fn new(service: Arc<QueueService>) -> Self {
        Self { service }
    }

    #[post("/join")]
    async fn join(&self, Json(req): JsonBody<QueueJoin>) -> Result<Response, ApiError> {
        join_inner(&self.service, req).await
    }

    #[get("/status/{ticket}")]
    async fn status(&self, Path(ticket): Path<String>) -> Result<Response, ApiError> {
        status_inner(&self.service, ticket).await
    }

    #[delete("/leave/{ticket}")]
    async fn leave(&self, Path(ticket): Path<String>) -> Result<Response, ApiError> {
        leave_inner(&self.service, ticket).await
    }
}
