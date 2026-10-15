use crate::PublishError;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

pub struct LiveSubscription<T> {
    namespace: &'static str,
    endpoint: &'static str,
    event: &'static str,
    client_name: &'static str,
    args: serde_json::Value,
    marker: PhantomData<fn() -> T>,
}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LiveSubscriptionMessage {
    pub endpoint: &'static str,
    pub args: serde_json::Value,
}

impl<T> LiveSubscription<T> {
    pub fn new<A: Serialize>(
        namespace: &'static str,
        endpoint: &'static str,
        event: &'static str,
        client_name: &'static str,
        args: A,
    ) -> Result<Self, PublishError> {
        Ok(Self {
            namespace,
            endpoint,
            event,
            client_name,
            args: serde_json::to_value(args)?,
            marker: PhantomData,
        })
    }
    pub fn namespace(&self) -> &'static str {
        self.namespace
    }
    pub fn endpoint(&self) -> &'static str {
        self.endpoint
    }
    pub fn event(&self) -> &'static str {
        self.event
    }
    pub fn client_name(&self) -> &'static str {
        self.client_name
    }
    pub fn args(&self) -> &serde_json::Value {
        &self.args
    }
    pub fn message(&self) -> LiveSubscriptionMessage {
        LiveSubscriptionMessage {
            endpoint: self.endpoint,
            args: self.args.clone(),
        }
    }
    pub fn decode(&self, payload: serde_json::Value) -> Result<T, serde_json::Error>
    where
        T: for<'de> Deserialize<'de>,
    {
        serde_json::from_value(payload)
    }
}
