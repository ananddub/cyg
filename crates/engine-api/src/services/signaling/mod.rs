use auto_di::singleton;

pub mod handshake;

pub struct SignalingService;

#[singleton]
impl SignalingService {
    fn new() -> Self {
        Self
    }
}
