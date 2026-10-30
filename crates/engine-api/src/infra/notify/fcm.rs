//! Firebase Cloud Messaging v1. The API takes an OAuth2 bearer token, and the
//! only way to get one from a service account on the server is the JWT-bearer
//! grant: sign an assertion with the account's key, exchange it for an access
//! token, and spend that token on the sends until it is nearly out of time.

use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::json;

use super::settings::Fcm;
use super::{CHANNEL_PUSH, Message, PROVIDER_FCM, Receipt, Reply, Vendor, http};

/// The one scope that lets a caller send, and nothing else.
const SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";

/// An assertion is good for an hour; it is exchanged again four minutes short of
/// that, so a token is never presented past its own expiry.
const SAFETY: Duration = Duration::from_secs(240);
const ASSERTION_TTL: Duration = Duration::from_secs(3_600);

pub struct Sender {
    settings: Fcm,
    /// The access token, when it was exchanged for, and how long it lasts.
    token: Mutex<Option<(String, Instant, Duration)>>,
}

impl Sender {
    pub fn new(settings: Fcm) -> Sender {
        Sender {
            settings,
            token: Mutex::new(None),
        }
    }

    fn cached(&self) -> Option<String> {
        let held = self
            .token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (token, minted, ttl) = held.as_ref()?;
        (minted.elapsed() + SAFETY < *ttl).then(|| token.clone())
    }

    fn remember(&self, token: String, ttl: Duration) {
        *self
            .token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((token, Instant::now(), ttl));
    }

    /// The service account's claim set: who is asking, of which authorisation
    /// server, for how much, and until when.
    fn assertion(&self) -> Result<String, String> {
        let now = now_secs();
        let key = EncodingKey::from_rsa_pem(self.settings.signing_key.as_bytes())
            .map_err(|err| format!("the service account key was refused: {err}"))?;
        let claims = json!({
            "iss": self.settings.client_email,
            "scope": SCOPE,
            "aud": self.settings.token_url,
            "iat": now,
            "exp": now + ASSERTION_TTL.as_secs(),
        });
        jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &key)
            .map_err(|err| format!("the assertion could not be signed: {err}"))
    }

    async fn access_token(&self) -> Result<String, String> {
        if let Some(token) = self.cached() {
            return Ok(token);
        }
        let assertion = self.assertion()?;
        let response = http()
            .post(self.settings.token_url.clone())
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", assertion.as_str()),
            ])
            .send()
            .await
            .map_err(|err| err.to_string())?;
        let code = response.status().as_u16();
        let body: serde_json::Value = response.json().await.map_err(|err| err.to_string())?;
        if !(200..300).contains(&code) {
            let error = body
                .get("error")
                .and_then(|value| value.as_str())
                .unwrap_or("no error named");
            return Err(format!("the token grant answered {code}: {error}"));
        }
        let token = body
            .get("access_token")
            .and_then(|value| value.as_str())
            .ok_or("the token grant answered without an access token")?
            .to_owned();
        let ttl = Duration::from_secs(
            body.get("expires_in")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(ASSERTION_TTL.as_secs()),
        );
        self.remember(token.clone(), ttl);
        Ok(token)
    }
}

impl Vendor for Sender {
    fn provider(&self) -> &'static str {
        PROVIDER_FCM
    }

    fn channel(&self) -> &'static str {
        CHANNEL_PUSH
    }

    fn send(&self, message: Message) -> Reply<'_> {
        Box::pin(async move {
            let token = self.access_token().await?;
            let body = json!({
                "message": {
                    "token": message.recipient,
                    "notification": {"title": message.title, "body": message.text},
                    "android": {"priority": "high"},
                    "apns": {"payload": {"aps": {"sound": "default"}}},
                },
            });
            let response = http()
                .post(format!(
                    "{}/v1/projects/{}/messages:send",
                    self.settings.api_host, self.settings.project
                ))
                .header("authorization", format!("Bearer {token}"))
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

/// Google's rejections come back as a `google.rpc.Status` body. An unregistered
/// token or a denied permission is a decision about *this* message; a 429 or a
/// 5xx is Firebase having a bad minute.
pub(crate) fn receipt(code: u16, body: &str) -> Receipt {
    let status = code == 429 || code >= 500;
    let named = body
        .split_once("\"status\"")
        .and_then(|(_, rest)| rest.split('"').nth(1))
        .unwrap_or("no status given");
    if (200..300).contains(&code) {
        Receipt::Accepted
    } else if status {
        Receipt::Later(format!("fcm answered {code}: {named}"))
    } else {
        Receipt::Refused(format!("fcm refused {code}: {named}"))
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
