use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::channel::{MessageResponse, SendRequest};
use crate::error::V2Error;
use crate::repositories::channel::InsertChannelMessageParams;
use crate::services::deps::q;

use crate::utils::channel::stream_of_channel;

use super::super::ChannelService;
use super::super::membership::assert_member;

impl ChannelService {
    pub async fn send_message(
        &self,
        state: &ApiState,
        auth: &Authed,
        channel_id: &str,
        body: SendRequest,
    ) -> Result<Json<MessageResponse>, V2Error> {
        let stream = stream_of_channel(channel_id)?;
        let uid = auth.0.uuid()?;
        assert_member(state, uid, &stream).await?;
        if !body.content.is_object() && !body.content.is_array() {
            return Err(V2Error::invalid_argument(
                "Message content must be a JSON object.",
            ));
        }
        let content = crate::moderation::sanitize_json(body.content);
        let msg = q(state).await?
            .insert_channel_message(&InsertChannelMessageParams {
                id: Uuid::new_v4(),
                sender_id: uid,
                username: auth.0.un.clone(),
                stream_mode: stream.mode,
                stream_subject: stream.subject,
                stream_descriptor: stream.descriptor,
                stream_label: stream.label.clone(),
                content,
            })
            .await?;
        Ok(Json(MessageResponse::new(&msg, &stream)))
    }
}
