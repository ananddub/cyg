use std::any::Any;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use super::extensions::{Extensions, Identity};

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

    pub fn send_line(&self, line: String) {
        let _ = self.inner.tx.send(line);
    }

    pub fn reply(&self, cid: Option<&str>, body: &Value) {
        let line = match (cid, body) {
            (None, body) => body.to_string(),
            (Some(cid), Value::Object(fields)) => {
                let mut object = serde_json::Map::with_capacity(fields.len() + 1);
                object.insert("cid".to_string(), Value::String(cid.to_string()));
                object.extend(
                    fields
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone())),
                );
                Value::Object(object).to_string()
            }
            (Some(cid), body) => serde_json::json!({ "cid": cid, "value": body }).to_string(),
        };
        self.send_line(line);
    }
}
