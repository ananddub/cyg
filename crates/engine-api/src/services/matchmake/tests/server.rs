use std::io::{Read, Write};
use std::net::SocketAddr;
use std::net::TcpStream;
use std::time::Duration;

use axum::Router;

use super::ApiState;

pub(super) struct TestServer {
    addr: SocketAddr,
    rt: Option<tokio::runtime::Runtime>,
}

impl TestServer {
    pub(super) fn start(state: ApiState) -> TestServer {
        Self::start_with(state, Router::new())
    }

    pub(super) fn start_with(state: ApiState, extra: Router) -> TestServer {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let routes = rt.block_on(crate::auto_router(state)).merge(extra);
        let listener = rt
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let app = routes.into_make_service_with_connect_info::<SocketAddr>();
        rt.spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        TestServer { addr, rt: Some(rt) }
    }

    pub(super) fn post(&self, path: &str, body: &str) -> (u16, Vec<(String, String)>, String) {
        let mut stream = TcpStream::connect(self.addr).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.addr,
            body.len()
        );
        stream.write_all(request.as_bytes()).unwrap();
        stream.flush().unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).unwrap();
        let text = String::from_utf8_lossy(&raw).into_owned();
        let (head, response_body) = text.split_once("\r\n\r\n").expect("response has a body");
        let status = head
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|c| c.parse().ok())
            .expect("status line");
        let headers = head
            .lines()
            .skip(1)
            .filter_map(|l| {
                l.split_once(':')
                    .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))
            })
            .collect();
        (status, headers, response_body.to_string())
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(rt) = self.rt.take() {
            rt.shutdown_timeout(Duration::from_secs(1));
        }
    }
}

pub(super) fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}
