use super::*;

pub struct PlainWs {
    pub name: String,
    pub ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
}
impl PlainWs {
    pub(crate) async fn connect(name: &str, url: String) -> PlainWs {
        let (ws, _) = tokio_tungstenite::connect_async(&url)
            .await
            .unwrap_or_else(|err| panic!("{name}: socket connects: {err}"));
        PlainWs {
            name: name.into(),
            ws,
        }
    }

    pub(crate) async fn next_json(&mut self, describe: &str) -> Value {
        loop {
            let msg = tokio::time::timeout(Duration::from_secs(10), self.ws.next())
                .await
                .unwrap_or_else(|_| panic!("{}: a frame within 10 s ({describe})", self.name));
            let Some(Ok(msg)) = msg else {
                panic!("{}: socket closed while waiting for {describe}", self.name);
            };
            match msg {
                Raw::Text(text) => {
                    return serde_json::from_str(text.as_str()).unwrap_or_else(|err| {
                        panic!("{}: frame is JSON: {err}: {text}", self.name)
                    });
                }
                Raw::Close(_) => panic!("{}: server closed before {describe}", self.name),
                _ => {}
            }
        }
    }

    pub(crate) async fn shutdown(mut self) {
        let _ = tokio::time::timeout(Duration::from_secs(2), self.ws.send(Raw::Close(None))).await;
    }
}
pub async fn expect_text(
    resp: reqwest::Response,
    status: StatusCode,
    what: &str,
) -> (String, String) {
    let actual = resp.status();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let body = resp.text().await.unwrap_or_default();
    assert_eq!(
        actual, status,
        "{what}: expected {status}, got {actual}: {body}"
    );
    (content_type, body)
}
pub async fn expect_bytes(resp: reqwest::Response, status: StatusCode, what: &str) -> Vec<u8> {
    let actual = resp.status();
    let body = resp.bytes().await.unwrap_or_default().to_vec();
    assert_eq!(actual, status, "{what}: expected {status}, got {actual}");
    body
}
pub fn expected_mime(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or_default() {
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "png" => "image/png",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after 1970")
        .as_secs()
}
pub async fn liveness(env: &Harness) {
    eprintln!("gap: edge liveness probes");
    let (typed, body) = expect_text(
        env.client
            .get(format!("{}/health", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /health",
    )
    .await;
    assert_eq!(body, "ok", "/health answers the literal");
    assert!(
        typed.starts_with("text/plain"),
        "/health is plain text: {typed}"
    );

    let auto = json_body(
        env.client
            .get(format!("{}/auto/health", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /auto/health",
    )
    .await;
    assert_eq!(auto["status"], "ok");
    assert_eq!(auto["routing"], "auto");
}
