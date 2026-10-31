use std::sync::Arc;

use auto_route::controller;
use axum::extract::Query;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::Response;

use crate::current_state;

use crate::dto::signaling::SignalQuery;
use crate::services::signaling::SignalingService;

type QueryParams<T> = Query<T>;

struct SignalingController {
    service: Arc<SignalingService>,
}

#[controller("/ws")]
impl SignalingController {
    fn new(service: Arc<SignalingService>) -> Self {
        Self { service }
    }

    #[get("/signal")]
    async fn signal(&self, query: QueryParams<SignalQuery>, ws: WebSocketUpgrade) -> Response {
        let state = current_state();
        self.service.signal(&state, query.0, ws)
    }
}
