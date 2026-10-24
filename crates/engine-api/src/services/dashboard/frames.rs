use engine_dashboard::WsFrame;

use super::{build_overview, build_ticks};
use crate::ApiState;

pub(crate) fn hello_frame(state: &ApiState) -> WsFrame {
    WsFrame::Hello {
        overview: Box::new(build_overview(state)),
        ticks: Box::new(build_ticks()),
        protocol: engine_dashboard::WS_PROTOCOL,
    }
}

pub(crate) fn snapshot_frame(state: &ApiState) -> WsFrame {
    WsFrame::Snapshot {
        overview: Box::new(build_overview(state)),
        ticks: Box::new(build_ticks()),
    }
}
