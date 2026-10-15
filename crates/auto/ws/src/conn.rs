use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

/// A per-connection type map.
///
/// The authenticator seeds it (the plane state, the verified claims) and the
/// `#[connect]` hook extends it (the session object, registries), which is how
/// handlers get hold of app objects without a second global.
#[derive(Default)]
pub struct Extensions {
    values: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Extensions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<T: Any + Send + Sync>(&mut self, value: T) {
        self.values.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.values.get(&TypeId::of::<T>())?.downcast_ref::<T>()
    }
}

/// What a successful upgrade knows about the caller.
pub struct Identity {
    pub user_id: Uuid,
    pub username: String,
    pub data: Extensions,
}

impl Identity {
    pub fn new(user_id: Uuid, username: impl Into<String>) -> Self {
        Self {
            user_id,
            username: username.into(),
            data: Extensions::new(),
        }
    }

    /// Carry an app object (the plane state, for one) into the connection's
    /// [`Conn`].
    pub fn with<T: Any + Send + Sync>(mut self, value: T) -> Self {
        self.data.insert(value);
        self
    }
}

/// One decoded inbound frame.
pub struct Msg {
    cid: Option<String>,
    event: String,
    payload: Value,
}

impl Msg {
    pub fn new(cid: Option<String>, event: impl Into<String>, payload: Value) -> Self {
        Self {
            cid,
            event: event.into(),
            payload,
        }
    }

    /// The request id to answer on, when the client sent one.
    pub fn cid(&self) -> Option<&str> {
        self.cid.as_deref()
    }

    /// The event tag [`Protocol::decode`](crate::Protocol::decode) routed on.
    pub fn event(&self) -> &str {
        &self.event
    }

    /// The whole payload, tag included.
    pub fn raw(&self) -> &Value {
        &self.payload
    }

    /// The payload as a typed value. A codec that validates the frame before
    /// handing it over makes this a formality; when it fails the failure is a
    /// parse error the client sees on the request's `cid`.
    pub fn payload<T: DeserializeOwned>(&self) -> Result<T, WsError> {
        serde_json::from_value(self.payload.clone()).map_err(|error| {
            WsError::new(format!("invalid payload for {}: {error}", self.event))
        })
    }
}

/// A failure the client sees as an error frame: `message` is what the protocol
/// puts on the wire.
#[derive(Debug, Clone)]
pub struct WsError {
    pub message: String,
}

impl WsError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for WsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for WsError {}

/// The per-connection handle every handler receives.
///
/// It owns the connection's writer, so anything a handler or a background
/// plane queues through it is delivered in order on the one socket task.
#[derive(Clone)]
pub struct Conn {
    inner: Arc<ConnInner>,
}

struct ConnInner {
    id: Uuid,
    user_id: Uuid,
    username: String,
    tx: UnboundedSender<String>,
    data: Mutex<Extensions>,
}

impl Conn {
    /// Built by the runtime; `id` is the session id the protocol will use in
    /// its own presence bookkeeping.
    pub fn new(id: Uuid, identity: Identity, tx: UnboundedSender<String>) -> Self {
        let Identity {
            user_id,
            username,
            data,
        } = identity;
        Self {
            inner: Arc::new(ConnInner {
                id,
                user_id,
                username,
                tx,
                data: Mutex::new(data),
            }),
        }
    }

    pub fn id(&self) -> Uuid {
        self.inner.id
    }

    pub fn user_id(&self) -> Uuid {
        self.inner.user_id
    }

    pub fn username(&self) -> &str {
        &self.inner.username
    }

    /// A clone of the writer, for the app's own session object so the planes
    /// can push to this socket.
    pub fn sender(&self) -> UnboundedSender<String> {
        self.inner.tx.clone()
    }

    pub fn get<T: Clone + Any + Send + Sync>(&self) -> Option<T> {
        self.inner
            .data
            .lock()
            .expect("connection data lock")
            .get::<T>()
            .cloned()
    }

    pub fn set<T: Any + Send + Sync>(&self, value: T) {
        self.inner
            .data
            .lock()
            .expect("connection data lock")
            .insert(value);
    }

    /// Queue a finished frame.
    pub fn send_line(&self, line: String) {
        let _ = self.inner.tx.send(line);
    }

    /// Answer `cid` with a message body that carries its own protocol tag.
    pub fn reply(&self, cid: Option<&str>, body: &Value) {
        let line = match (cid, body) {
            (None, body) => body.to_string(),
            (Some(cid), Value::Object(fields)) => {
                let mut object = serde_json::Map::with_capacity(fields.len() + 1);
                object.insert("cid".to_string(), Value::String(cid.to_string()));
                object.extend(fields.iter().map(|(key, value)| (key.clone(), value.clone())));
                Value::Object(object).to_string()
            }
            (Some(cid), body) => serde_json::json!({ "cid": cid, "value": body }).to_string(),
        };
        self.send_line(line);
    }
}
