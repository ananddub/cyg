pub(crate) const SPECTATOR_CONFIRM: &[u8] =
    &[0xA5, b'd', b'e', b'l', b't', b'a', 0, 0, 0, 0, 0, 0, 0, 0];

pub(crate) const MAX_FRAMES_PER_LOOP: usize = 256;

pub(crate) const APP_MESSAGE_OPCODE: u8 = 0x05;

pub(crate) const CLOSING_EVENT: &str = "server_closing";

pub(crate) const RUDP_DEDUPE_SPAN: usize = 256;

mod boot;
mod claims;
mod dispatch_events;
mod drain;
mod engine;
mod history;
mod hot_detect;
mod hot_run;
mod inbound_dispatch;
mod inbound_events;
mod loop_transport;
mod on_connected;
mod on_disconnected;
mod packet_input;
mod packet_route;
mod packet_signal;
mod packet_voice;
mod ratelimit;
mod rehydrate;
mod report;
mod run;
mod server_boot;
mod step;
mod step_signals;
mod telemetry;
mod tick_delta;
mod tick_fanout;
mod tick_gate;
mod tick_legacy;
mod tick_legacy_players;
mod tick_legacy_udp;
mod tick_schema_senders;
mod tick_schema_sessions;
mod udp_join;
mod udp_msg;
mod udp_run;
mod wire;

#[allow(unused_imports)]
pub(crate) use engine::{
    Clock, Engine, HotReload, Rates, Scratch, Stats, TickCtx, TickRunner, UdpCtx, UdpLane,
};
#[allow(unused_imports)]
pub(crate) use loop_transport::DelayedOut;
pub use loop_transport::LoopTransport;
pub use run::run;

#[allow(unused_imports)]
pub(crate) use {
    boot::*, claims::*, drain::*, history::*, ratelimit::*, rehydrate::*, report::*,
    server_boot::*, wire::*,
};

#[allow(unused_imports)]
pub(crate) use crate::netcode;
#[allow(unused_imports)]
pub(crate) use crate::rooms::{
    CloseReason, DRAIN_REASON, GameRoom, GameServer, OutboundMessage, RoomBackend, RoomHostData,
};
#[allow(unused_imports)]
pub(crate) use crate::schema_view::SchemaAoi;
#[allow(unused_imports)]
pub(crate) use crate::telemetry::{PublishReport, Recorder};
#[allow(unused_imports)]
pub(crate) use engine_api::infra::signaling::{SignalHub, SignalIn};
#[allow(unused_imports)]
pub(crate) use engine_bus::Bus;
#[allow(unused_imports)]
pub(crate) use engine_config::{BootMaps, BootModule, GameConfig, GameplayConfig, MapFormat};
#[allow(unused_imports)]
pub(crate) use engine_core::{MessageRateLimits, RateDecision};
#[allow(unused_imports)]
pub(crate) use engine_net::framing::OpCode;
#[allow(unused_imports)]
pub(crate) use engine_net::transport::webrtc::contract::Inbound;
#[allow(unused_imports)]
pub(crate) use engine_net::transport::{
    GameTransport, TransportEvent, WebRtcServer, WebRtcTransport,
};
#[allow(unused_imports)]
pub(crate) use engine_wasm::{EpochTicker, WasmRuntime};
#[allow(unused_imports)]
pub(crate) use std::collections::{HashMap, VecDeque};
#[allow(unused_imports)]
pub(crate) use std::net::SocketAddr;
#[allow(unused_imports)]
pub(crate) use std::path::{Path, PathBuf};
#[allow(unused_imports)]
pub(crate) use std::sync::Arc;
#[allow(unused_imports)]
pub(crate) use std::time::{Duration, Instant, SystemTime};

#[cfg(test)]
mod tests;
