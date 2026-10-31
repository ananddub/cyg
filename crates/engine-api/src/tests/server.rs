use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::ApiState;
use crate::error::ErrorKind;

pub(super) struct V2Server {
    addr: SocketAddr,
    rt: Option<tokio::runtime::Runtime>,
}

pub(super) struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Reply {
    pub(super) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub(super) fn refusal(&self, kind: ErrorKind) -> String {
        let value = self.json();
        assert_eq!(self.status, kind.status().as_u16(), "body: {}", self.body);
        assert_eq!(
            value["error"]["code"].as_u64(),
            Some(u64::from(kind.code())),
            "refusal of kind {:?} reports the wrong code in {}",
            kind,
            self.body
        );
        value["error"]["message"]
            .as_str()
            .unwrap_or_else(|| panic!("no message in {}", self.body))
            .to_owned()
    }

    pub(super) fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("body is not JSON: {e}\n{}", self.body))
    }
}

impl V2Server {
    pub(super) fn start(state: ApiState) -> V2Server {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let routes = rt.block_on(crate::auto_router(state));
        let listener = rt
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let addr = listener.local_addr().unwrap();
        rt.spawn(async move {
            let _ = axum::serve(listener, routes).await;
        });
        V2Server { addr, rt: Some(rt) }
    }

    pub(super) fn post(&self, path: &str, auth: Option<&str>, body: &str) -> Reply {
        self.send("POST", path, auth, body)
    }

    pub(super) fn get(&self, path: &str, auth: Option<&str>) -> Reply {
        self.send("GET", path, auth, "")
    }

    pub(super) fn put(&self, path: &str, auth: Option<&str>, body: &str) -> Reply {
        self.send("PUT", path, auth, body)
    }

    pub(super) fn delete(&self, path: &str, auth: Option<&str>) -> Reply {
        self.send("DELETE", path, auth, "")
    }

    pub(super) fn send(&self, method: &str, path: &str, auth: Option<&str>, body: &str) -> Reply {
        let mut stream = TcpStream::connect(self.addr).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let credential = auth
            .map(|value| format!("Authorization: {value}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\n{credential}\
             Content-Type: application/json\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            self.addr,
            body.len()
        );
        stream.write_all(request.as_bytes()).unwrap();
        stream.flush().unwrap();

        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).unwrap();
        let text = String::from_utf8_lossy(&raw).into_owned();
        let (head, response_body) = text.split_once("\r\n\r\n").expect("reply has a body");
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .expect("status line");
        let headers = head
            .lines()
            .skip(1)
            .filter_map(|line| {
                line.split_once(':')
                    .map(|(name, value)| (name.trim().to_lowercase(), value.trim().to_owned()))
            })
            .collect();
        let value: serde_json::Value = serde_json::from_str(response_body).unwrap_or_else(|e| {
            panic!("{path} -> {status} body is not JSON: {e}\n{response_body}")
        });
        if status >= 400 {
            assert!(
                value["error"]["message"]
                    .as_str()
                    .is_some_and(|m| !m.is_empty()),
                "{path} -> {status} refusal has no message: {response_body}"
            );
        }
        Reply {
            status,
            headers,
            body: response_body.to_owned(),
        }
    }
}

impl Drop for V2Server {
    fn drop(&mut self) {
        if let Some(rt) = self.rt.take() {
            rt.shutdown_timeout(Duration::from_secs(1));
        }
    }
}
