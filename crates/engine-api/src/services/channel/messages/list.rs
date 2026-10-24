//! `ListChannelMessages` — the keyset page read.

use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::channel::{ListQuery, MessageListResponse, MessageResponse};
use crate::error::V2Error;
use crate::repositories::channel::{
    ListChannelMessagesBackwardParams, ListChannelMessagesForwardParams,
};
use crate::services::auth::q;
use crate::services::social::{ts_head, ts_tail};

use crate::utils::channel::{
    BAD_CURSOR, LIMIT_RANGE, decode_cursor, encode_cursor, stream_of_channel,
};

use super::super::ChannelService;
use super::super::membership::assert_member;

impl ChannelService {
    /// `GET /v2/channel/{channel_id}?limit=&cursor=&forward=` — the
    /// `ListChannelMessages`.
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
        // One row past the page is read; that row is what makes a next cursor.
        let limit = page + 1;
        let forward = req.forward.unwrap_or(true);
        let cursor = decode_cursor(req.cursor.as_deref())?;
        if let Some(c) = &cursor {
            // A cursor belongs to the stream and direction that issued it.
            let same = c.stream.mode == stream.mode
                && c.stream.subject == stream.subject
                && c.stream.descriptor == stream.descriptor
                && c.stream.label == stream.label;
            if !same || c.forward != forward {
                return Err(V2Error::invalid_argument(BAD_CURSOR));
            }
        }
        // The direction stored in the cursor decides how the page is read, and a
        // previous-page cursor is the only case where the rows are flipped back.
        let ascending = match &cursor {
            None => forward,
            Some(c) => (forward && c.next) || (!forward && !c.next),
        };
        let flip = cursor.as_ref().is_some_and(|c| !c.next);

        let queries = q(state)?;
        // The keyset bound is the cursor's position, or an extreme that admits the
        // whole stream when the caller asked for the first page.
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

        // The extra row only says a page follows; the cursors name the ends of the
        // page as it was read, so they are taken before the rows are flipped.
        let over_read = rows.len() as i64 > page;
        if over_read {
            rows.pop();
        }
        let head = rows.first().map(|m| (m.create_time, m.id));
        let tail = rows.last().map(|m| (m.create_time, m.id));
        let after = if over_read { tail } else { None };
        let before = if cursor.is_some() { head } else { None };
        // A previous-page listing was read the other way round, so its "next" names
        // where the read started and its "previous" names the page before that.
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
            // An empty page still renews the cursor it was asked with.
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
