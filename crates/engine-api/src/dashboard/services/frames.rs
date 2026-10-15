
use engine_dashboard::WsFrame;

use crate::ApiState;
use crate::dashboard::services;

pub(crate) fn hello_frame(state: &ApiState) -> WsFrame {
    WsFrame::Hello {
        overview: Box::new(services::build_overview(state)),
        ticks: Box::new(services::build_ticks()),
        protocol: engine_dashboard::WS_PROTOCOL,
    }
}

pub(crate) fn snapshot_frame(state: &ApiState) -> WsFrame {
    WsFrame::Snapshot {
        overview: Box::new(services::build_overview(state)),
        ticks: Box::new(services::build_ticks()),
    }
}
