use serde::Serialize;

use crate::models::log::LogLine;
use crate::models::{Overview, TickSeries};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum WsFrame {
    Hello {
        overview: Box<Overview>,
        ticks: Box<TickSeries>,
        protocol: u32,
    },
    Snapshot {
        overview: Box<Overview>,
        ticks: Box<TickSeries>,
    },
    /// One change in an account-plane table, scoped to the socket's game. A
    /// page refetches what it shows; the frame carries no row of its own.
    Live {
        topic: String,
        action: String,
        record: String,
        seq: u64,
    },
    /// A batch of mirrored log lines, delivered only to sockets that asked.
    Logs {
        lines: Vec<LogLine>,
    },
    Goodbye {
        reason: String,
    },
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
        let s = serde_json::to_value(WsFrame::Goodbye {
            reason: "plane shutting down".into(),
        })
        .unwrap();
        assert_eq!(s["type"].as_str(), Some("goodbye"));
        // The dashboard's parser keys off these four fields and nothing else: a
        // live frame is an invalidation, never a row.
        let live = serde_json::to_value(WsFrame::Live {
            topic: "users".into(),
            action: "create".into(),
            record: "abc".into(),
            seq: 7,
        })
        .unwrap();
        assert_eq!(live["type"].as_str(), Some("live"));
        assert_eq!(live["topic"].as_str(), Some("users"));
        assert_eq!(live["action"].as_str(), Some("create"));
        assert_eq!(live["record"].as_str(), Some("abc"));
        assert_eq!(live["seq"].as_u64(), Some(7));
        assert!(live.get("overview").is_none());
    }
}
