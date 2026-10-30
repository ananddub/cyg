use axum::Json;
use chrono::Utc;
use engine_config::Rotation;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::{Authed, ConsoleAuthed};
use crate::dto::notify::{
    AttemptResponse, PreferenceListResponse, PreferenceRequest, PreferenceResponse, TestRequest,
    UnsubscribeResponse,
};
use crate::error::V2Error;
use crate::infra::notify::{CHANNEL_EMAIL, CHANNEL_PUSH, KIND_ALL, Registry, channel_of, token};
use crate::repositories::models::NotificationPreference;
use crate::repositories::notify::preference_id;
use crate::services::console::{audit, id};
use crate::services::deps::q;
use crate::services::quest::catalog::definition_key;

use super::NotifyService;

/// The two channels there is anything to switch. A name that is neither is a
/// client bug, and saying so is better than writing a row nobody will read.
fn channel(raw: &str) -> Result<&'static str, V2Error> {
    match raw.trim() {
        CHANNEL_PUSH => Ok(CHANNEL_PUSH),
        CHANNEL_EMAIL => Ok(CHANNEL_EMAIL),
        _ => Err(V2Error::invalid_argument(
            "A notification channel is push or email.",
        )),
    }
}

/// One kind's name, or `*` for the whole channel. Empty means the same as `*`:
/// a client that only offers a channel-sized switch should not have to invent a
/// kind to say "everything".
fn kind(raw: &str) -> Result<String, V2Error> {
    let raw = raw.trim();
    if raw.is_empty() || raw == KIND_ALL {
        return Ok(KIND_ALL.to_owned());
    }
    definition_key(raw, "notification kind")
}

pub(crate) fn preference_row(
    user_id: Uuid,
    channel: &str,
    kind: &str,
    enabled: bool,
) -> NotificationPreference {
    NotificationPreference {
        id: preference_id(user_id, channel, kind),
        user_id,
        channel: channel.to_owned(),
        kind: kind.to_owned(),
        enabled,
        update_time: Utc::now(),
    }
}

impl NotifyService {
    /// What this account has spoken about. A kind with no row is not here, and
    /// means it is being sent.
    pub async fn list(
        &self,
        state: &ApiState,
        auth: &Authed,
    ) -> Result<Json<PreferenceListResponse>, V2Error> {
        let rows = q(state).await?.list_preferences(auth.uid()).await?;
        Ok(Json(PreferenceListResponse {
            success: true,
            preferences: rows.into_iter().map(|row| row.into()).collect(),
        }))
    }

    /// Records one choice. The row is keyed by account, channel and kind, so
    /// this is the whole of it: there is nothing to create first.
    pub async fn set(
        &self,
        state: &ApiState,
        auth: &Authed,
        body: PreferenceRequest,
    ) -> Result<Json<PreferenceResponse>, V2Error> {
        let row = preference_row(
            auth.uid(),
            channel(&body.channel)?,
            &kind(&body.kind)?,
            body.enabled,
        );
        q(state).await?.put_preference(&row).await.map(|_| ())?;
        Ok(Json(PreferenceResponse {
            success: true,
            preference: row.into(),
        }))
    }

    /// The one write a recipient can make without signing in, because the proof
    /// of who they are arrives signed inside the link their mail carried.
    pub async fn unsubscribe(
        &self,
        state: &ApiState,
        presented: &str,
    ) -> Result<Json<UnsubscribeResponse>, V2Error> {
        let previous = &Rotation::global().previous_seat_secret;
        let target =
            token::verify(&state.control.seat_secret, previous, presented).ok_or_else(|| {
                V2Error::invalid_argument(
                    "This unsubscribe link is not one this plane signed, or it has expired.",
                )
            })?;
        let name = channel(&target.channel)?;
        let row = preference_row(target.user_id, name, &target.kind, false);
        q(state).await?.put_preference(&row).await.map(|_| ())?;
        Ok(Json(UnsubscribeResponse {
            success: true,
            channel: name.to_owned(),
            message: if name == CHANNEL_EMAIL {
                "No more mail will be sent to this address.".to_owned()
            } else {
                "No more pushes will be sent to this account.".to_owned()
            },
        }))
    }

    /// Puts one message from the console onto the queue. It answers what it
    /// wrote rather than what happened next: asking whether a relay is
    /// reachable is never a reason to hold a request open.
    pub async fn test(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        body: TestRequest,
    ) -> Result<Json<AttemptResponse>, V2Error> {
        claims.require_write()?;
        let provider = body.provider.trim().to_owned();
        let recipient = body.recipient.trim().to_owned();
        if recipient.is_empty() || recipient.len() > 320 {
            return Err(V2Error::invalid_argument(
                "A delivery test needs a recipient to send to.",
            ));
        }
        // An unconfigured provider is refused here rather than queued and left
        // to die quietly in the loop: the operator asked a question and this is
        // the only place the answer is still useful.
        let registry = Registry::from_env();
        if let Some(reason) = registry.refusal(&provider) {
            return Err(V2Error::unavailable(reason));
        }
        let Some(name) = channel_of(&provider) else {
            return Err(V2Error::unavailable(format!(
                "There is no sender built for {provider}."
            )));
        };
        let row = super::outbox::test_attempt(&provider, name, recipient);
        q(state).await?.queue_attempt(&row).await?;
        audit(
            state,
            claims,
            "notify.test",
            "delivery_attempt",
            Some(row.id),
            &format!("{provider} to {}", row.recipient),
        )
        .await;
        Ok(Json(AttemptResponse {
            success: true,
            attempt: (&row).into(),
        }))
    }

    /// What became of one queued message, which is how a refused provider
    /// surfaces after the test that was queued around it.
    pub async fn attempt(
        &self,
        state: &ApiState,
        _claims: &ConsoleAuthed,
        raw: &str,
    ) -> Result<Json<AttemptResponse>, V2Error> {
        let found = q(state)
            .await?
            .attempt(id(raw, "delivery")?)
            .await?
            .ok_or_else(|| V2Error::not_found("There is no delivery with that id."))?;
        Ok(Json(AttemptResponse {
            success: true,
            attempt: (&found).into(),
        }))
    }
}
