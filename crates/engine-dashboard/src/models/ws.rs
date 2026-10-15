use serde::Serialize;

use crate::models::{Overview, TickSeries};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum WsFrame {
    Hello { overview: Box<Overview>, ticks: Box<TickSeries>, protocol: u32 },
    Snapshot { overview: Box<Overview>, ticks: Box<TickSeries> },
    Goodbye { reason: String },
}

pub const WS_PROTOCOL: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ws_frame_is_self_describing() {
        let f = WsFrame::Hello {
            overview: Box::new(Overview::default()),
            ticks: Box::new(TickSeries::default()),
            protocol: WS_PROTOCOL,
        };
        let json = serde_json::to_value(&f).unwrap();
        assert_eq!(json["type"].as_str(), Some("hello"));
        assert_eq!(json["protocol"].as_u64(), Some(1));
        assert!(json["overview"].is_object());
        let s = serde_json::to_value(WsFrame::Goodbye { reason: "plane shutting down".into() }).unwrap();
        assert_eq!(s["type"].as_str(), Some("goodbye"));
    }
}
