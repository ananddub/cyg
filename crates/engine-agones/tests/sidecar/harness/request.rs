use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

#[derive(Debug, Clone)]
pub(crate) struct Request {
    pub method: String,
    pub path: String,
    pub version: String,
    pub head: String,
}

pub(crate) fn read_request(stream: &mut TcpStream) -> Option<Request> {
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .ok();
    let mut buf = [0u8; 1024];
    let mut head = Vec::new();
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                head.extend_from_slice(&buf[..n]);
                if head.windows(4).any(|w| w == b"\r\n\r\n") || head.len() > 8192 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    if head.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(&head).into_owned();
    let first = text.lines().next().unwrap_or("").to_string();
    let mut fields = first.split_whitespace();
    Some(Request {
        method: fields.next().unwrap_or("").to_string(),
        path: fields.next().unwrap_or("").to_string(),
        version: fields.next().unwrap_or("").to_string(),
        head: text,
    })
}

pub(crate) fn closed_port() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind to find a free port");
    let addr = listener.local_addr().expect("local addr");
    drop(listener);
    addr.to_string()
}
