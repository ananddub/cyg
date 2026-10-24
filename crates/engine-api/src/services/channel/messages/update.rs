//! Message update.

use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::channel::{MessageResponse, UpdateRequest};
use crate::error::V2Error;
use crate::repositories::channel::UpdateChannelMessageParams;
use crate::services::auth::q;

use crate::utils::channel::stream_of_channel;

use super::super::ChannelService;
use super::super::membership::assert_member;

impl ChannelService {
    pub async fn update_message(
        &self,
        state: &ApiState,
        auth: &Authed,
        channel_id: &str,
        message_id: &str,
        body: UpdateRequest,
    ) -> Result<Json<MessageResponse>, V2Error> {
        let stream = stream_of_channel(channel_id)?;
        assert_member(state, auth.0.uuid()?, &stream).await?;
        let id = Uuid::parse_str(message_id)
            .map_err(|_| V2Error::invalid_argument("Invalid message ID."))?;
        let msg = q(state)?
            .update_channel_message(&UpdateChannelMessageParams {
                content: body.content,
                id,
                sender_id: auth.0.uuid()?,
            })
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => V2Error::not_found("Channel message not found."),
                other => V2Error::from_sqlx(other),
            })?;
        Ok(Json(MessageResponse::new(&msg, &stream)))
    }
}
