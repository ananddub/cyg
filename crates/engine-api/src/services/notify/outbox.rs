use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::ApiState;
use crate::infra::liveops;
use crate::infra::notify::{
    CHANNEL_EMAIL, CHANNEL_PUSH, Face, KIND_ALL, PROVIDER_APNS, PROVIDER_FCM, PROVIDER_SMTP,
    Registry, template, token,
};
use crate::repositories::db::Queries;
use crate::repositories::models::{DeliveryAttempt, Notification, NotificationPreference};
use crate::repositories::notify::QUEUED;
use crate::services::social;

/// The outside world's copies of one in-app notification. This writes rows and
/// nothing else: the vendors are the delivery loop's business, so a request
/// never waits on a relay and a relay that is down cannot fail the request that
/// made the fact true.
///
/// Nothing is queued for a channel with no configured vendor, which is what
/// makes the whole surface inert until an operator puts credentials in.
pub(crate) async fn enqueue(state: &ApiState, queries: &Arc<Queries>, note: &Notification) {
    let users = match queries.users_by_id(&[note.user_id]).await {
        Ok(users) => users,
        Err(err) => {
            tracing::warn!("[notify] the account could not be read: {err}");
            return;
        }
    };
    // An erased account is owed nothing outside the app, whatever the rows still
    // sitting in its inbox say, and a barred one is not to be reached either.
    let Some(user) = users.get(&note.user_id) else {
        return;
    };
    if user.disable_time.timestamp() != 0 {
        return;
    }
    let kind = social::kind_of(note.code);
    let rows = queries
        .list_preferences(note.user_id)
        .await
        .unwrap_or_default();
    let registry = Registry::from_env();
    let face = Face::read();
    let mut owed: Vec<(&'static str, &'static str, String)> = Vec::new();
    if permitted(&rows, CHANNEL_PUSH, kind) {
        let devices = queries.push_tokens(note.user_id).await.unwrap_or_default();
        push_target(
            &registry,
            PROVIDER_APNS,
            devices.iter().map(|row| row.push_token_ios.as_str()),
            &mut owed,
        );
        push_target(
            &registry,
            PROVIDER_FCM,
            devices.iter().map(|row| row.push_token_android.as_str()),
            &mut owed,
        );
    }
    if permitted(&rows, CHANNEL_EMAIL, kind)
        && registry.vendor(PROVIDER_SMTP).is_some()
        // Mail goes to the address on the account, so it goes only once the
        // account has confirmed the address is really its own.
        && user.verify_time.timestamp() != 0
    {
        if let Some(address) = user
            .email
            .as_deref()
            .map(str::trim)
            .filter(|address| !address.is_empty())
        {
            owed.push((CHANNEL_EMAIL, PROVIDER_SMTP, address.to_owned()));
        }
    }
    for (channel, provider, recipient) in owed {
        let (subject, text) = match channel {
            CHANNEL_EMAIL => (
                note.subject.clone(),
                template::email(kind, &note.subject, &footer(state, &face, note.user_id)),
            ),
            _ => (face.push_title.clone(), note.subject.clone()),
        };
        let now = Utc::now();
        let row = DeliveryAttempt {
            id: Uuid::new_v4(),
            user_id: note.user_id,
            notification_id: note.id,
            channel: channel.to_owned(),
            provider: provider.to_owned(),
            recipient,
            subject,
            text,
            status: QUEUED.to_owned(),
            attempts: 0,
            error: String::new(),
            next_retry_at: now,
            create_time: now,
            update_time: now,
        };
        if let Err(err) = queries.queue_attempt(&row).await {
            tracing::warn!("[notify] {} could not be queued: {err}", note.id);
        }
    }
}

/// The queued row for one console delivery test. It is owed to no account, so
/// it carries the nil one: a console that hammer-tests a relay spends the same
/// shared window a player would, and no player's budget pays for that.
pub(crate) fn test_attempt(
    provider: &str,
    channel: &'static str,
    recipient: String,
) -> DeliveryAttempt {
    let now = Utc::now();
    DeliveryAttempt {
        id: Uuid::new_v4(),
        user_id: Uuid::nil(),
        notification_id: Uuid::new_v4(),
        channel: channel.to_owned(),
        provider: provider.to_owned(),
        recipient,
        subject: "Notification delivery test".to_owned(),
        text: "The console asked this plane to send one message, to check the delivery path."
            .to_owned(),
        status: QUEUED.to_owned(),
        attempts: 0,
        error: String::new(),
        next_retry_at: now,
        create_time: now,
        update_time: now,
    }
}

/// One push token per provider per notification: the first device the account
/// registered for that platform, not every device it has ever had. A
/// notification that reaches three old phones is one notification three times.
fn push_target<'a>(
    registry: &Registry,
    provider: &'static str,
    tokens: impl Iterator<Item = &'a str>,
    owed: &mut Vec<(&'static str, &'static str, String)>,
) {
    if registry.vendor(provider).is_none() {
        return;
    }
    if let Some(token) = tokens.map(str::trim).find(|token| !token.is_empty()) {
        owed.push((CHANNEL_PUSH, provider, token.to_owned()));
    }
}

/// The footer link, or nothing when the operator has not said where this plane
/// is reachable from the outside. It asks for the whole channel to stop: one
/// click in a mail client should not have to go looking for the right switch.
fn footer(state: &ApiState, face: &Face, user_id: Uuid) -> String {
    if face.link_base.is_empty() {
        return String::new();
    }
    let days = liveops::config().notify_unsubscribe_days;
    let signed = token::mint(
        &state.control.seat_secret,
        &token::Unsubscribe {
            user_id,
            channel: CHANNEL_EMAIL.to_owned(),
            kind: KIND_ALL.to_owned(),
        },
        days.saturating_mul(86_400),
    );
    format!("{}/v2/notify/unsubscribe?token={signed}", face.link_base)
}

/// The account's own row for the kind, then the channel's wildcard, then yes. A
/// notification type nobody has ever written a row about is not thereby switched
/// off, which is why there is no row needed to switch it on.
pub(crate) fn permitted(rows: &[NotificationPreference], channel: &str, kind: &str) -> bool {
    let find = |name: &str| {
        rows.iter()
            .find(|row| row.channel == channel && row.kind == name)
    };
    find(kind)
        .or_else(|| find(KIND_ALL))
        .is_none_or(|row| row.enabled)
}
