mod gamecenter;
mod token;
mod unsigned;

use axum::Json;

use crate::ApiState;
use crate::dto::auth::ProviderAccount;
use crate::dto::session::SessionResponse;
use crate::services::auth::verify;

pub use gamecenter::game_center;
pub(crate) use gamecenter::game_center_fields;
pub use unsigned::instant_game;
pub(crate) use token::token;

pub(crate) async fn provider(
    state: &ApiState,
    account: ProviderAccount,
    requested_username: &str,
    create: bool,
) -> Result<Json<SessionResponse>, crate::error::V2Error> {
    let Some(kind) = verify::Kind::parse(&account.provider) else {
        return unsigned::provider(&account.provider, requested_username);
    };
    let token = account
        .payload
        .as_ref()
        .and_then(|payload| payload.get("token"))
        .and_then(|token| token.as_str())
        .unwrap_or_default()
        .to_owned();
    token::token(state, kind, &token, requested_username, create, account.vars).await
}
