//! The outside world's half of a notification: what a message looks like, who
//! can carry it, and what they said about the one attempt just made.
//!
//! A vendor is built out of the environment and only when the settings it
//! cannot work without are all there, so an unconfigured plane has no vendor,
//! queues nothing, and says so with the names of the settings it wants rather
//! than pretending a message went out.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

use crate::infra::liveops;

pub mod apns;
pub mod fcm;
mod settings;
pub mod smtp;
pub mod template;
pub mod token;

pub use settings::Face;

/// One pool for every vendor call this process makes. The registry is rebuilt
/// per beat, because reading the environment is free next to the network call
/// that follows it; a connection pool is not something to rebuild.
pub(crate) fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_millis(liveops::config().notify_timeout_ms))
            .build()
            .unwrap_or_default()
    })
}

pub const CHANNEL_PUSH: &str = "push";
pub const CHANNEL_EMAIL: &str = "email";
pub const PROVIDER_APNS: &str = "apns";
pub const PROVIDER_FCM: &str = "fcm";
pub const PROVIDER_SMTP: &str = "smtp";
/// A preference that speaks for every kind on its channel.
pub const KIND_ALL: &str = "*";

/// What a vendor said about one attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Receipt {
    /// The vendor took it. Nothing more is owed.
    Accepted,
    /// Worth trying again: the vendor was busy, the network gave up, or the
    /// rejection was one that a later attempt can fix.
    Later(String),
    /// The vendor will never take this one — a token it no longer recognises,
    /// an address it bounces, credentials it refuses. Retrying is only noise.
    Refused(String),
}

/// One message, rendered and ready for the wire. The unsubscribe footer is
/// already in `text`: a vendor should not have to know what a preference is.
#[derive(Debug, Clone)]
pub struct Message {
    /// The device token or the mail address.
    pub recipient: String,
    pub title: String,
    pub text: String,
}

/// A future is boxed rather than `async fn` in the trait: the registry hands
/// out `Arc<dyn Vendor>`, and an async trait method is not object safe.
type Reply<'a> = Pin<Box<dyn Future<Output = Result<Receipt, String>> + Send + 'a>>;

pub trait Vendor: Send + Sync {
    fn provider(&self) -> &'static str;
    fn channel(&self) -> &'static str;
    /// `Err` is the transport giving out, which the worker owes another try.
    fn send(&self, message: Message) -> Reply<'_>;
}

/// The vendors this process can actually reach. Rebuilt per beat and per
/// request, because it is a handful of environment reads next to the network
/// call it precedes.
pub struct Registry {
    vendors: Vec<Arc<dyn Vendor>>,
}

impl Registry {
    pub fn from_env() -> Registry {
        let mut vendors: Vec<Arc<dyn Vendor>> = Vec::new();
        if let Ok(settings) = settings::Apns::read() {
            vendors.push(Arc::new(apns::Sender::new(settings)));
        }
        if let Ok(settings) = settings::Fcm::read() {
            vendors.push(Arc::new(fcm::Sender::new(settings)));
        }
        if let Ok(settings) = settings::Smtp::read() {
            vendors.push(Arc::new(smtp::Sender::new(settings)));
        }
        Registry { vendors }
    }

    pub fn vendor(&self, provider: &str) -> Option<Arc<dyn Vendor>> {
        self.vendors
            .iter()
            .find(|vendor| vendor.provider() == provider)
            .cloned()
    }

    /// Why this provider cannot carry a message, in the words an operator can
    /// act on. `None` means it can.
    pub fn refusal(&self, provider: &str) -> Option<String> {
        if self.vendor(provider).is_some() {
            return None;
        }
        let reason = match provider {
            PROVIDER_APNS => settings::Apns::read().err(),
            PROVIDER_FCM => settings::Fcm::read().err(),
            PROVIDER_SMTP => settings::Smtp::read().err(),
            _ => None,
        };
        Some(match reason {
            Some(reason) => format!("{} is not configured, {reason}.", label(provider)),
            None => format!("There is no sender built for {provider}."),
        })
    }
}

/// Which channel a provider carries, so a caller that named a provider can
/// record the row on the right channel.
pub fn channel_of(provider: &str) -> Option<&'static str> {
    match provider {
        PROVIDER_APNS | PROVIDER_FCM => Some(CHANNEL_PUSH),
        PROVIDER_SMTP => Some(CHANNEL_EMAIL),
        _ => None,
    }
}

/// `smtp` is a protocol, not a product; the message an operator reads should
/// name the thing they have to go and get credentials for.
fn label(provider: &str) -> &'static str {
    match provider {
        PROVIDER_APNS => "Apple push",
        PROVIDER_FCM => "Firebase push",
        PROVIDER_SMTP => "Email",
        _ => "This sender",
    }
}
