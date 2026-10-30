//! Apple's push service, as the team's provider token rather than per-device
//! certificates: a short-lived ES256 JWT the same signing key mints for every
//! send, cached for the hour Apple says it is worth.

use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::json;

use super::settings::Apns;
use super::{CHANNEL_PUSH, Message, PROVIDER_APNS, Receipt, Reply, Vendor, http};

/// Apple refuses a provider token older than an hour; it is remade well inside
/// that, so a long beat never races the expiry.
const TOKEN_TTL: Duration = Duration::from_secs(2_400);

pub struct Sender {
    settings: Apns,
    token: Mutex<Option<(String, Instant)>>,
}

impl Sender {
    pub fn new(settings: Apns) -> Sender {
        Sender {
            settings,
            token: Mutex::new(None),
        }
    }

    /// Signing a JWT is arithmetic, not a round trip, so this takes no await
    /// and the cache lock never has to be held across one.
    fn provider_token(&self) -> Result<String, String> {
        let mut cached = self
            .token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((token, minted)) = cached.as_ref()
            && minted.elapsed() < TOKEN_TTL
        {
            return Ok(token.clone());
        }
        let key = EncodingKey::from_ec_pem(self.settings.signing_key.as_bytes())
            .map_err(|err| format!("the APNs signing key was refused: {err}"))?;
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(self.settings.key_id.clone());
        let claims = json!({
            "iss": self.settings.team,
            "iat": now_secs(),
        });
        let token = jsonwebtoken::encode(&header, &claims, &key)
            .map_err(|err| format!("the provider token could not be signed: {err}"))?;
        *cached = Some((token.clone(), Instant::now()));
        Ok(token)
    }
}

impl Vendor for Sender {
    fn provider(&self) -> &'static str {
        PROVIDER_APNS
    }

    fn channel(&self) -> &'static str {
        CHANNEL_PUSH
    }

    fn send(&self, message: Message) -> Reply<'_> {
        Box::pin(async move {
            let token = self.provider_token()?;
            let body = json!({
                "aps": {
                    "alert": {"title": message.title, "body": message.text},
                    "sound": "default",
                },
            });
            let response = http()
                .post(format!(
                    "{}/3/device/{}",
                    self.settings.host, message.recipient
                ))
                .header("authorization", format!("bearer {token}"))
                .header("apns-topic", self.settings.topic.clone())
                .header("apns-push-type", "alert")
                .header("apns-priority", "10")
                .json(&body)
                .send()
                .await
                .map_err(|err| err.to_string())?;
            let code = response.status().as_u16();
            let reason = response.text().await.unwrap_or_default();
            Ok(receipt(code, &reason))
        })
    }
}

/// APNs answers a rejection with `{"reason": "BadDeviceToken"}`, which is the
/// only part of the body worth keeping in the row's error column. 410 means the
/// token is gone for good, and every other 4xx is Apple telling us the message
/// or the credentials are wrong — neither gets better by asking again.
pub(crate) fn receipt(code: u16, body: &str) -> Receipt {
    let reason = body
        .split_once("\"reason\"")
        .and_then(|(_, rest)| rest.split('"').nth(1))
        .unwrap_or("no reason given");
    if (200..300).contains(&code) {
        Receipt::Accepted
    } else if code == 408 || code == 429 || code >= 500 {
        Receipt::Later(format!("apns answered {code}: {reason}"))
    } else {
        Receipt::Refused(format!("apns refused {code}: {reason}"))
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
