//! The handshake the one `/ws/signal` route performs once the transport is done.

use auto_di::singleton;

pub mod handshake;

pub struct SignalingService;

#[singleton]
impl SignalingService {
    fn new() -> Self {
        Self
    }
}
