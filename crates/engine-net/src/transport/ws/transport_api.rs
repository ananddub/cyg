use std::borrow::Cow;

use bytes::Bytes;

use super::error::MatchMakeError;
use super::heartbeat::HeartbeatConfig;
use super::transport::WsTransport;

impl WsTransport {
    /// Room variants for filtered matchmaking on this lane.
    pub fn set_room_variants(
        &mut self,
        variants: std::collections::BTreeMap<String, engine_config::RoomVariantDef>,
    ) {
        self.room_variants = variants;
    }

    pub fn set_schema_descriptor(&mut self, descriptor: impl Into<Bytes>) {
        self.schema_descriptor = descriptor.into();
    }

    pub fn set_udp_port(&mut self, port: u16) {
        self.advertised_udp_port = port;
    }

    pub fn set_heartbeat(&mut self, config: HeartbeatConfig) {
        self.heartbeat = config;
    }

    pub fn set_allowed_origins(&mut self, origins: Vec<String>) {
        self.allowed_origins = origins;
    }

    pub fn on_http(
        &mut self,
        f: impl Fn(&str) -> Option<(Cow<'static, str>, Bytes)> + Send + 'static,
    ) {
        self.http_hook = Some(Box::new(f));
    }

    pub fn on_matchmake(
        &mut self,
        f: impl Fn(&str, &str, &str) -> Result<String, MatchMakeError> + Send + 'static,
    ) {
        self.matchmake_hook = Some(Box::new(f));
    }

    pub fn room_of(&self, client_id: &str) -> Option<&str> {
        self.clients_meta.get(client_id)?.room.as_deref()
    }

    pub fn options_of(&self, client_id: &str) -> Option<&str> {
        self.clients_meta.get(client_id)?.options.as_deref()
    }

    pub fn room_of_shared(&self, client_id: &str) -> Option<std::sync::Arc<str>> {
        self.clients_meta.get(client_id)?.room.clone()
    }

    pub fn options_of_shared(&self, client_id: &str) -> Option<std::sync::Arc<str>> {
        self.clients_meta.get(client_id)?.options.clone()
    }

    pub fn take_consented(&mut self, client_id: &str) -> bool {
        self.consented.remove(client_id)
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }

    pub fn retain_session(&mut self, room_id: &str, session_id: &str) {
        self.retained.insert(
            format!("{}:{}", room_id, session_id),
            session_id.to_string(),
        );
    }
}
