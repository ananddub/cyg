use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::channel::{ListQuery, MessageListResponse, MessageResponse};
use crate::error::V2Error;
use crate::repositories::channel::{
    ListChannelMessagesBackwardParams, ListChannelMessagesForwardParams,
};
use crate::services::deps::q;
use crate::services::social::{ts_head, ts_tail};

use crate::utils::channel::{
    BAD_CURSOR, LIMIT_RANGE, decode_cursor, encode_cursor, stream_of_channel,
};

use super::super::ChannelService;
use super::super::membership::assert_member;

impl ChannelService {
    pub async fn list_messages(
        &self,
        state: &ApiState,
        auth: &Authed,
        channel_id: &str,
        req: ListQuery,
    ) -> Result<Json<MessageListResponse>, V2Error> {
        let stream = stream_of_channel(channel_id)?;
        assert_member(state, auth.0.uuid()?, &stream).await?;

        let page = match req.limit {
            None => 1,
            Some(n) if (1..=100).contains(&n) => n,
            Some(_) => return Err(V2Error::invalid_argument(LIMIT_RANGE)),
        };
        let limit = page + 1;
        let forward = req.forward.unwrap_or(true);
        let cursor = decode_cursor(req.cursor.as_deref())?;
        if let Some(c) = &cursor {
            let same = c.stream.mode == stream.mode
                && c.stream.subject == stream.subject
                && c.stream.descriptor == stream.descriptor
                && c.stream.label == stream.label;
            if !same || c.forward != forward {
                return Err(V2Error::invalid_argument(BAD_CURSOR));
            }
        }
        let ascending = match &cursor {
            None => forward,
            Some(c) => (forward && c.next) || (!forward && !c.next),
        };
        let flip = cursor.as_ref().is_some_and(|c| !c.next);

        let queries = q(state).await?;
        let (create_time, id) =
            cursor
                .as_ref()
                .map(|c| (c.create_time, c.id))
                .unwrap_or(if ascending {
                    (ts_head(), Uuid::nil())
                } else {
                    (ts_tail(), Uuid::max())
                });

        let mut rows = if ascending {
            queries
                .list_channel_messages_forward(&ListChannelMessagesForwardParams {
                    stream_mode: stream.mode,
                    stream_subject: stream.subject,
                    stream_descriptor: stream.descriptor,
                    stream_label: stream.label.clone(),
                    create_time,
                    id,
                    limit,
                })
                .await?
        } else {
            queries
                .list_channel_messages_backward(&ListChannelMessagesBackwardParams {
                    stream_mode: stream.mode,
                    stream_subject: stream.subject,
                    stream_descriptor: stream.descriptor,
                    stream_label: stream.label.clone(),
                    create_time,
                    id,
                    limit,
                })
                .await?
        };

        let over_read = rows.len() as i64 > page;
        if over_read {
            rows.pop();
        }
        let head = rows.first().map(|m| (m.create_time, m.id));
        let tail = rows.last().map(|m| (m.create_time, m.id));
        let after = if over_read { tail } else { None };
        let before = if cursor.is_some() { head } else { None };
        let (next, prev) = if flip {
            (before, after)
        } else {
            (after, before)
        };
        if flip {
            rows.reverse();
        }

        let messages: Vec<MessageResponse> = rows
            .iter()
            .map(|m| MessageResponse::new(m, &stream))
            .collect();
        let next_cursor =
            next.map(|(create_time, id)| encode_cursor(&stream, create_time, id, forward, true));
        let prev_cursor =
            prev.map(|(create_time, id)| encode_cursor(&stream, create_time, id, forward, false));
        let cacheable_cursor = if let Some(m) = rows.last() {
            Some(encode_cursor(&stream, m.create_time, m.id, true, true))
        } else if let Some(c) = &cursor {
            Some(encode_cursor(
                &stream,
                c.create_time,
                c.id,
                true,
                !forward || c.next,
            ))
        } else {
            None
        };
        Ok(Json(MessageListResponse {
            messages,
            next_cursor,
            prev_cursor,
            cacheable_cursor,
        }))
    }
}
