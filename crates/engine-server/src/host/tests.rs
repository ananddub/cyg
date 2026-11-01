use super::*;

    fn transport() -> LoopTransport {
        LoopTransport::new(
            WebRtcTransport::new("127.0.0.1:0".parse().unwrap()).unwrap(),
            None,
            None,
        )
    }

    fn message(event: &str) -> OutboundMessage {
        OutboundMessage {
            target: 7,
            event: event.to_string(),
            payload: vec![1, 2, 3],
        }
    }

mod console_module;
mod event_history_claims;
mod event_history_rows;
mod loop_transport_basics;
mod loop_transport_drain;
mod loop_transport_ownership;
