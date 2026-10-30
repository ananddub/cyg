use super::*;

pub fn test_module_bytes() -> Vec<u8> {
    let names = engine_api::services::wasm::REQUIRED_EXPORTS;
    let mut module = vec![0x00, b'a', b's', b'm', 0x01, 0x00, 0x00, 0x00];

    let mut types = leb128(1);
    types.extend([0x60u8, 0x00, 0x00]);
    module.extend(section(1, &types));

    let functions: Vec<u8> = leb128(names.len())
        .into_iter()
        .chain(std::iter::repeat_n(0u8, names.len()))
        .collect();
    module.extend(section(3, &functions));

    let mut exports = leb128(names.len());
    for (index, name) in names.iter().enumerate() {
        exports.extend(leb128(name.len()));
        exports.extend(name.as_bytes());
        exports.push(0x00);
        exports.extend(leb128(index));
    }
    module.extend(section(7, &exports));

    let mut bodies = leb128(names.len());
    for _ in names {
        bodies.extend(leb128(2));
        bodies.extend([0x00, 0x0b]);
    }
    module.extend(section(10, &bodies));
    module
}
pub fn leb128(mut value: usize) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            return out;
        }
    }
}
pub fn section(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![id];
    out.extend(leb128(payload.len()));
    out.extend(payload);
    out
}
pub struct TestWsClient {
    pub name: String,
    pub ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    pub n: u32,
    pub pending: Vec<Value>,
}
impl TestWsClient {
    pub(crate) async fn connect(port: u16, token: &str, name: &str) -> Self {
        let url = format!("ws://127.0.0.1:{port}/ws?token={token}");
        let (ws, _) = tokio_tungstenite::connect_async(&url)
            .await
            .unwrap_or_else(|err| panic!("{name}: socket connects: {err}"));
        let mut client = Self {
            name: name.into(),
            ws,
            n: 0,
            pending: Vec::new(),
        };
        client.liveness().await;
        client
    }

    async fn liveness(&mut self) {
        let cid = self.next_cid();
        self.send_raw(json!({"cid": cid, "type": "ping"}).to_string())
            .await;
        let pong = self
            .take_reply(&cid, Some("pong"), Duration::from_secs(5))
            .await;
        assert_eq!(pong["type"], "pong");
    }

    fn next_cid(&mut self) -> String {
        self.n += 1;
        format!("{}-{}", self.name, self.n)
    }

    async fn send_raw(&mut self, text: String) {
        self.ws
            .send(Raw::Text(text.into()))
            .await
            .expect("frame sent");
    }

    pub(crate) async fn send_event(&mut self, tag: &str, payload: Value) -> String {
        let cid = self.next_cid();
        let mut frame = json!({"cid": cid, "type": tag});
        if let Value::Object(map) = payload {
            for (key, value) in map {
                frame[key] = value;
            }
        }
        self.send_raw(frame.to_string()).await;
        cid
    }

    pub(crate) async fn fire(&mut self, tag: &str, payload: Value) {
        self.send_event(tag, payload).await;
    }

    async fn take_where(
        &mut self,
        want: &dyn Fn(&Value) -> bool,
        describe: &str,
        timeout_dur: Duration,
    ) -> Value {
        if let Some(index) = self.pending.iter().position(want) {
            return self.pending.remove(index);
        }
        let deadline = tokio::time::Instant::now() + timeout_dur;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                panic!("{}: timeout waiting for {describe}", self.name);
            }
            let msg = tokio::time::timeout(remaining, self.ws.next())
                .await
                .unwrap_or_else(|_| panic!("{}: timeout waiting for {describe}", self.name));
            let Some(Ok(Raw::Text(text))) = msg else {
                panic!(
                    "{}: socket closed mid-read while waiting for {describe}",
                    self.name
                );
            };
            let frame: Value = serde_json::from_str(&text).expect("frame is JSON");
            if want(&frame) {
                return frame;
            }
            self.pending.push(frame);
        }
    }

    pub(crate) async fn take_reply(
        &mut self,
        cid: &str,
        want_type: Option<&str>,
        timeout_dur: Duration,
    ) -> Value {
        let want = want_type.map(str::to_owned);
        let cid = cid.to_owned();
        self.take_where(
            &|frame| {
                frame["cid"] == cid.as_str() && want.as_deref().is_none_or(|t| frame["type"] == t)
            },
            &format!("reply to cid {cid}"),
            timeout_dur,
        )
        .await
    }

    pub(crate) async fn take_event(&mut self, want_type: &str, timeout_dur: Duration) -> Value {
        let want = want_type.to_owned();
        self.take_where(
            &|frame| frame["type"] == want,
            &format!("event {want}"),
            timeout_dur,
        )
        .await
    }
}
