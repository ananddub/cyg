mod engine;
mod stream;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;

use quinn_proto::{
    Connection, ConnectionError, ConnectionHandle, Endpoint, EndpointConfig, ServerConfig,
};

#[derive(Debug)]
pub enum QuicEvent {
    Connected(ConnectionHandle),

    Lost(ConnectionHandle, ConnectionError),

    StreamData {
        handle: ConnectionHandle,
        id: quinn_proto::StreamId,
        bytes: Vec<u8>,
    },

    StreamFinished {
        handle: ConnectionHandle,
        id: quinn_proto::StreamId,
    },

    StreamOpened {
        handle: ConnectionHandle,
        id: quinn_proto::StreamId,
    },

    Datagram {
        handle: ConnectionHandle,
        bytes: Vec<u8>,
    },
}

pub struct QuicEngine {
    endpoint: Endpoint,
    server_config: Option<Arc<ServerConfig>>,
    connections: HashMap<ConnectionHandle, Connection>,
    pending: VecDeque<quinn_proto::Incoming>,
    outbox: VecDeque<(SocketAddr, Vec<u8>)>,

    buf: Vec<u8>,
}

fn read_stream(
    conn: &mut Connection,
    handle: ConnectionHandle,
    id: quinn_proto::StreamId,
    events: &mut Vec<QuicEvent>,
) {
    let mut recv = conn.recv_stream(id);
    let mut chunks = match recv.read(true) {
        Ok(chunks) => chunks,
        Err(_) => return,
    };
    loop {
        let chunk = match chunks.next(64 * 1024) {
            Ok(Some(chunk)) => chunk,
            _ => break,
        };
        let bytes = chunk.bytes.to_vec();
        events.push(QuicEvent::StreamData { handle, id, bytes });
    }

    let _ = chunks.finalize();
}

pub fn frame_bytes(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

pub fn endpoint_config() -> EndpointConfig {
    let rng = ring::rand::SystemRandom::new();
    let key =
        ring::hmac::Key::generate(ring::hmac::HMAC_SHA256, &rng).expect("ring hmac key generation");
    EndpointConfig::new(Arc::new(key))
}

pub fn self_signed_server_config() -> Result<ServerConfig, Box<dyn std::error::Error>> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    let pair = rcgen::generate_simple_self_signed(vec!["cyg.local".to_owned()])?;
    let cert_der: CertificateDer<'static> = pair.cert.der().to_owned();
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pair.signing_key.serialize_der()));
    Ok(ServerConfig::with_single_cert(vec![cert_der], key_der)?)
}
