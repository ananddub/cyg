use super::*;

impl WsClient {
    pub(crate) async fn connect(port: u16, token: &str, name: &str, uid: Uuid, user: &str) -> WsClient {
        let url = format!("ws://127.0.0.1:{port}/ws?token={token}");
        let (ws, _) = tokio_tungstenite::connect_async(&url)
            .await
            .unwrap_or_else(|err| panic!("{name}: socket connects: {err}"));
        let mut client = WsClient {
            name: name.into(),
            user: user.into(),
            uid,
            ws,
            n: 0,
            pending: Vec::new(),
        };
        client.liveness().await;
        client
    }

    async fn liveness(&mut self) {
        let cid = self.next_cid();
        self.send(json!({"cid": cid, "type": "ping"}).to_string())
            .await;
        let pong = self.take_reply(&cid, Some("pong"), "ping liveness").await;
        assert_eq!(pong["type"], "pong");
    }

    fn next_cid(&mut self) -> String {
        self.n += 1;
        format!("{}{}", self.name, self.n)
    }

    async fn send(&mut self, text: String) {
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
        self.send(frame.to_string()).await;
        cid
    }

    pub(crate) async fn fire(&mut self, tag: &str, payload: Value) {
        self.send_event(tag, payload).await;
    }

    async fn read_frame(&mut self) -> Option<Value> {
        loop {
            let msg = tokio::time::timeout(Duration::from_secs(10), self.ws.next())
                .await
                .unwrap_or_else(|_| panic!("{}: a frame within 10 s", self.name))?;
            let Ok(msg) = msg else {
                panic!("{}: socket closed mid-read", self.name);
            };
            if let Raw::Text(text) = msg {
                return Some(serde_json::from_str(text.as_str()).expect("frame is JSON"));
            }
        }
    }

    fn find(&self, want: &dyn Fn(&Value) -> bool) -> Option<usize> {
        self.pending.iter().position(want)
    }

    async fn take_where(&mut self, want: &dyn Fn(&Value) -> bool, describe: &str) -> Value {
        if let Some(index) = self.find(want) {
            return self.pending.remove(index);
        }
        loop {
            let frame = match self.read_frame().await {
                Some(frame) => frame,
                None => panic!("{}: socket closed while waiting for {describe}", self.name),
            };
            if want(&frame) {
                return frame;
            }
            self.pending.push(frame);
        }
    }

    pub(crate) async fn take_reply(&mut self, cid: &str, want_type: Option<&str>, describe: &str) -> Value {
        let want = want_type.map(str::to_owned);
        let cid = cid.to_owned();
        self.take_where(
            &|frame| {
                frame["cid"] == cid.as_str() && want.as_deref().is_none_or(|t| frame["type"] == t)
            },
            &format!("{describe} (cid {cid})"),
        )
        .await
    }

    pub(crate) async fn take_event(&mut self, want_type: &str) -> Value {
        let want = want_type.to_owned();
        self.take_where(&|frame| frame["type"] == want, &format!("event {want}"))
            .await
    }

    pub(crate) async fn take_ws_error(&mut self, cid: &str, describe: &str) -> String {
        let frame = self
            .take_where(
                &|f| f["type"] == "error" && f["cid"] == cid,
                &format!("{describe} (error on cid {cid})"),
            )
            .await;
        frame["message"].as_str().unwrap_or_default().to_owned()
    }

    pub(crate) async fn drain(&mut self, window: Duration) {
        loop {
            let next = tokio::time::timeout(window, self.ws.next()).await;
            match next {
                Ok(Some(Ok(Raw::Text(text)))) => {
                    if let Ok(frame) = serde_json::from_str::<Value>(text.as_str()) {
                        self.pending.push(frame);
                    }
                }
                _ => break,
            }
        }
    }

    pub(crate) async fn expect_quiet_for(&mut self, window: Duration, describe: &str) {
        if let Ok(next) = tokio::time::timeout(window, self.ws.next()).await {
            match next {
                Some(Ok(Raw::Text(text))) => {
                    let frame: Value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
                    panic!(
                        "{}: unexpected frame {frame} while expecting {describe}",
                        self.name
                    );
                }
                Some(Ok(_)) => {}
                Some(Err(err)) => panic!("{}: socket error: {err}", self.name),
                None => panic!("{}: socket closed while expecting {describe}", self.name),
            }
        }
    }

    pub(crate) async fn close(mut self) {
        let _ = self.ws.close(None).await;
        while let Ok(Some(msg)) = tokio::time::timeout(Duration::from_secs(5), self.ws.next()).await
        {
            if msg.is_err() || matches!(msg, Ok(Raw::Close(_))) {
                break;
            }
        }
    }
}
pub fn mint(uid: Uuid, username: &str) -> String {
    engine_api::infra::jwt::mint_token_pair(uid, username, HashMap::new()).0
}
pub fn uid_of(player: &Player) -> Uuid {
    Uuid::parse_str(&player.uid).expect("player uid is a uuid")
}
