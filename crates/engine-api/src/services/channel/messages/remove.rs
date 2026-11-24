use axum::Json;
use uuid::Uuid;

use crate::app::middleware::Authed;
use crate::db::DbError;
use crate::dto::channel::MessageResponse;
use crate::error::V2Error;

use crate::utils::channel::stream_of_channel;

use super::super::ChannelService;
use super::super::membership::assert_member;

impl ChannelService {
    pub async fn delete_message(
        &self,

        auth: &Authed,
        channel_id: &str,
        message_id: &str,
    ) -> Result<Json<MessageResponse>, V2Error> {
        let stream = stream_of_channel(channel_id)?;
        assert_member(&self.plane, auth.0.uuid()?, &stream).await?;
        let id = Uuid::parse_str(message_id)
            .map_err(|_| V2Error::invalid_argument("Invalid message ID."))?;
        let msg = self
            .plane
            .q()
            .await?
            .delete_channel_message(id, auth.0.uuid()?)
            .await
            .map_err(|e| match e {
                DbError::NotFound => V2Error::not_found("Channel message not found."),
                other => V2Error::from(other),
            })?;
        Ok(Json(MessageResponse::new(&msg, &stream)))
    }
}
