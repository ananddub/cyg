//! Request extraction that keeps the `/v2` error contract intact: the gateway
//! always decodes JSON (unknown fields dropped, an empty body tolerated) and
//! answers every parse failure with a 400 `{"message": …}` — axum's own extractors
//! would answer plain text instead.

use axum::extract::FromRequest;
use axum::extract::FromRequestParts;
use axum::extract::Query;
use axum::extract::Request;
use serde::de::DeserializeOwned;

use crate::error::V2Error;

/// The gateway's own body cap; the same 2 MiB axum applies by default.
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

/// A JSON body, absent or empty meaning "no fields set".
///
/// The gateway's `err != io.EOF` test is why an empty body is legal; a bare `null`
/// is legal because the gateway reads it as "leave the message unchanged", so the
/// field checks run on zero values, as Go's nil-safe `in.GetX()` accessors do.
pub struct Body<T>(pub T);

impl<S, T> FromRequest<S> for Body<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Default,
{
    type Rejection = V2Error;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let bytes = axum::body::to_bytes(req.into_body(), MAX_BODY_BYTES)
            .await
            .map_err(|_| invalid("Request body too large."))?;
        if bytes.is_empty() || bytes.trim_ascii() == b"null" {
            return Ok(Body(T::default()));
        }
        serde_json::from_slice(&bytes)
            .map(Body)
            .map_err(|e| invalid(&format!("Invalid request body: {e}")))
    }
}

/// The query string, with values parsed the way the gateway's field filter does.
pub struct Params<T>(pub T);

impl<S, T> FromRequestParts<S> for Params<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = V2Error;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        match Query::<T>::from_request_parts(parts, state).await {
            Ok(Query(value)) => Ok(Params(value)),
            Err(e) => Err(invalid(&e.body_text())),
        }
    }
}

fn invalid(message: &str) -> V2Error {
    V2Error::invalid_argument(message.to_owned())
}

/// `create` is a nullable boolean carried in the query string, so it arrives as
/// text and is parsed by Go's `strconv.ParseBool`. Being nullable is what makes
/// *absent* mean `true` rather than `false`.
#[derive(Debug, Clone, Copy, Default)]
pub enum BoolFlag {
    #[default]
    Unset,
    True,
    False,
}

impl BoolFlag {
    /// `None` in the query means "the client did not say", which BaaS reads
    /// as create-on-miss.
    pub fn create(&self) -> bool {
        !matches!(self, BoolFlag::False)
    }

    /// The accepted spellings are `ParseBool`'s exact set; anything else is a
    /// bad request rather than a silently ignored one.
    pub fn parse(raw: &str) -> Result<BoolFlag, V2Error> {
        match raw {
            "1" | "t" | "T" | "TRUE" | "true" | "True" => Ok(BoolFlag::True),
            "0" | "f" | "F" | "FALSE" | "false" | "False" => Ok(BoolFlag::False),
            other => Err(invalid(&format!(
                "invalid query parameter, error: cannot parse \"{other}\" as a boolean value since it does not match valid options \"true || false, or 1 || 0, or t || f, or T || F, or True || False, or TRUE || FALSE\""
            ))),
        }
    }
}
