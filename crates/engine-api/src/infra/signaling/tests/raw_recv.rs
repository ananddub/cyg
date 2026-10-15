//! Frame decoding half of the raw client.

use super::raw_ws::{WsClient, read_byte, read_chunk};

impl WsClient {
    /// The next text frame, or `None` once the server closes or hangs up; pings
    /// are ponged and control noise is skipped.
    pub(super) async fn recv(&mut self) -> Option<String> {
        loop {
            let b0 = read_byte(&mut self.io).await?;
            let b1 = read_byte(&mut self.io).await?;
            assert_eq!(b1 & 0x80, 0, "the server must not mask its frames");
            let opcode = b0 & 0x0f;
            let mut len = (b1 & 0x7f) as u64;
            if len == 126 {
                let mut x = [0u8; 2];
                read_chunk(&mut self.io, &mut x).await?;
                len = u16::from_be_bytes(x) as u64;
            } else if len == 127 {
                let mut x = [0u8; 8];
                read_chunk(&mut self.io, &mut x).await?;
                len = u64::from_be_bytes(x);
                assert!(len < 1 << 20, "absurd frame length {len}");
            }
            let mut payload = vec![0u8; len as usize];
            if len > 0 {
                read_chunk(&mut self.io, &mut payload).await?;
            }
            match opcode {
                0x1 => return String::from_utf8(payload).ok(),
                0x8 => return None,
                0x9 => self.send_frame(0xA, &payload).await,
                _ => {}
            }
        }
    }

    pub(super) async fn recv_json(&mut self) -> serde_json::Value {
        serde_json::from_str(&self.recv().await.expect("expected a text frame")).unwrap()
    }
}
