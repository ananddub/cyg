use axum::http::request::Parts;
use axum::response::Response;

use crate::{Identity, Msg, WsError};

pub trait Protocol: Send + Sync + 'static {

    fn authenticate(parts: &Parts) -> Result<Identity, Response>;

    fn decode(frame: &str) -> Result<Msg, WsError>;

    fn encode_error(cid: Option<&str>, error: &WsError) -> String;
}
