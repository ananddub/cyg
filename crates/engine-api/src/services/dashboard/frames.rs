use crate::services::plane::Plane;

use engine_dashboard::WsFrame;

use super::{build_overview, build_ticks};

pub(crate) fn hello_frame(plane: &Plane) -> WsFrame {
    WsFrame::Hello {
        overview: Box::new(build_overview(plane)),
        ticks: Box::new(build_ticks()),
        protocol: engine_dashboard::WS_PROTOCOL,
    }
}

pub(crate) fn snapshot_frame(plane: &Plane) -> WsFrame {
    WsFrame::Snapshot {
        overview: Box::new(build_overview(plane)),
        ticks: Box::new(build_ticks()),
    }
}
