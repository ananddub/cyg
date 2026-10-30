//! What the outside world is reachable at, read from the environment when a
//! sender is built.
//!
//! A vendor is *configured* when every setting it cannot work without has a
//! value, and nothing else. That is what makes the whole surface inert by
//! default: a plane with no credentials queues no messages, refuses with the
//! names of the settings it wants, and behaves exactly as it did before any of
//! this existed.

pub(crate) fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default().trim().to_owned()
}

/// A setting the operator may leave alone because there is a sane value for the
/// public service, and must override to point at something else.
fn env_or(name: &str, default: &str) -> String {
    let value = env(name);
    if value.is_empty() {
        default.to_owned()
    } else {
        value
    }
}

fn missing(names: &[&'static str]) -> String {
    format!("set {}", names.join(", "))
}

/// Apple's push service, authenticated as this team's provider token.
pub struct Apns {
    pub team: String,
    pub key_id: String,
    /// The PKCS#8 signing key, as PEM text.
    pub signing_key: String,
    pub topic: String,
    pub host: String,
}

const APNS_KEYS: [&str; 4] = [
    "OX_PUSH_APNS_TEAM",
    "OX_PUSH_APNS_KEY_ID",
    "OX_PUSH_APNS_KEY_PEM",
    "OX_PUSH_APNS_TOPIC",
];

impl Apns {
    pub fn read() -> Result<Apns, String> {
        let row = Apns {
            team: env("OX_PUSH_APNS_TEAM"),
            key_id: env("OX_PUSH_APNS_KEY_ID"),
            signing_key: env("OX_PUSH_APNS_KEY_PEM"),
            topic: env("OX_PUSH_APNS_TOPIC"),
            host: env_or("OX_PUSH_APNS_HOST", "https://api.push.apple.com"),
        };
        let empty: Vec<&'static str> = APNS_KEYS
            .iter()
            .zip([
                row.team.as_str(),
                row.key_id.as_str(),
                row.signing_key.as_str(),
                row.topic.as_str(),
            ])
            .filter(|(_, value)| value.is_empty())
            .map(|(name, _)| *name)
            .collect();
        if empty.is_empty() {
            Ok(row)
        } else {
            Err(missing(&empty))
        }
    }
}

/// Firebase Cloud Messaging, v1 API, whose calls take an OAuth2 access token
/// minted from a service account's key.
pub struct Fcm {
    pub project: String,
    pub client_email: String,
    pub signing_key: String,
    pub token_url: String,
    pub api_host: String,
}

const FCM_KEYS: [&str; 3] = [
    "OX_PUSH_FCM_PROJECT",
    "OX_PUSH_FCM_CLIENT_EMAIL",
    "OX_PUSH_FCM_SERVICE_ACCOUNT_PEM",
];

impl Fcm {
    pub fn read() -> Result<Fcm, String> {
        let row = Fcm {
            project: env("OX_PUSH_FCM_PROJECT"),
            client_email: env("OX_PUSH_FCM_CLIENT_EMAIL"),
            signing_key: env("OX_PUSH_FCM_SERVICE_ACCOUNT_PEM"),
            token_url: env_or(
                "OX_PUSH_FCM_TOKEN_URL",
                "https://oauth2.googleapis.com/token",
            ),
            api_host: env_or("OX_PUSH_FCM_API_HOST", "https://fcm.googleapis.com"),
        };
        let empty: Vec<&'static str> = FCM_KEYS
            .iter()
            .zip([
                row.project.as_str(),
                row.client_email.as_str(),
                row.signing_key.as_str(),
            ])
            .filter(|(_, value)| value.is_empty())
            .map(|(name, _)| *name)
            .collect();
        if empty.is_empty() {
            Ok(row)
        } else {
            Err(missing(&empty))
        }
    }
}

/// How the connection to a relay is encrypted. `none` is for a relay on this
/// machine's own loopback, which is what a test runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Plain,
    StartTls,
    Implicit,
}

pub struct Smtp {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
    pub ehlo: String,
    pub transport: Transport,
}

impl Smtp {
    pub fn read() -> Result<Smtp, String> {
        let row = Smtp {
            host: env("OX_EMAIL_HOST"),
            port: env("OX_EMAIL_PORT").parse::<u16>().unwrap_or(
                match env("OX_EMAIL_TLS").as_str() {
                    "implicit" => 465,
                    "none" => 25,
                    _ => 587,
                },
            ),
            username: env("OX_EMAIL_USERNAME"),
            password: env("OX_EMAIL_PASSWORD"),
            from: env("OX_EMAIL_FROM"),
            ehlo: env_or("OX_EMAIL_DOMAIN", "localhost"),
            transport: match env("OX_EMAIL_TLS").as_str() {
                "none" => Transport::Plain,
                "implicit" => Transport::Implicit,
                _ => Transport::StartTls,
            },
        };
        if row.host.is_empty() || row.from.is_empty() {
            Err(missing(&["OX_EMAIL_HOST", "OX_EMAIL_FROM"]))
        } else {
            Ok(row)
        }
    }
}

/// The part of an outgoing mail that is not the relay: the label a push arrives
/// under and the base the unsubscribe link is written onto.
pub struct Face {
    pub push_title: String,
    pub link_base: String,
}

impl Face {
    pub fn read() -> Face {
        Face {
            push_title: env_or("OX_NOTIFY_TITLE", "Notifications"),
            link_base: env("OX_NOTIFY_UNSUBSCRIBE_BASE")
                .trim_end_matches('/')
                .to_owned(),
        }
    }
}
