use std::sync::Arc;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// What a real receiver has to do with a delivery: read the raw bytes, check the
/// signature over the timestamp and those bytes, refuse a stale one, and answer
/// with a status the worker can act on.
pub struct Hit {
    pub body: String,
    pub signature: String,
    pub timestamp: String,
    pub delivery: String,
    pub valid: bool,
}

#[derive(Clone)]
pub struct Receiver {
    pub url: String,
    secret: Arc<String>,
    hits: Arc<Mutex<Vec<Hit>>>,
    status: Arc<AtomicU16>,
    delay_ms: Arc<AtomicU64>,
}

impl Receiver {
    pub async fn answering(status: u16, secret: &str) -> Receiver {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback receiver port");
        let port = listener.local_addr().expect("receiver address").port();
        let receiver = Receiver {
            url: format!("http://127.0.0.1:{port}/hook"),
            secret: Arc::new(secret.to_owned()),
            hits: Arc::new(Mutex::new(Vec::new())),
            status: Arc::new(AtomicU16::new(status)),
            delay_ms: Arc::new(AtomicU64::new(0)),
        };
        let (secret, hits, code, delay) = (
            receiver.secret.clone(),
            receiver.hits.clone(),
            receiver.status.clone(),
            receiver.delay_ms.clone(),
        );
        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    return;
                };
                let (secret, hits) = (secret.clone(), hits.clone());
                let code = code.load(Ordering::SeqCst);
                let delay = delay.load(Ordering::SeqCst);
                tokio::spawn(async move {
                    let _ = serve(socket, &secret, &hits, code, delay).await;
                });
            }
        });
        receiver
    }

    /// Stalls before answering, which is how a slow endpoint is told apart from
    /// a slow API.
    pub fn set_delay(&self, ms: u64) {
        self.delay_ms.store(ms, Ordering::SeqCst);
    }

    pub fn hits(&self) -> usize {
        self.hits.lock().expect("the receiver log").len()
    }

    /// The most recent delivery, so the test can check the signature itself.
    pub fn last(&self) -> Hit {
        let log = self.hits.lock().expect("the receiver log");
        let hit = log.last().expect("the receiver took a delivery");
        Hit {
            body: hit.body.clone(),
            signature: hit.signature.clone(),
            timestamp: hit.timestamp.clone(),
            delivery: hit.delivery.clone(),
            valid: hit.valid,
        }
    }

    pub async fn wait_for(&self, want: usize) -> bool {
        for _ in 0..300 {
            if self.hits() >= want {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        false
    }
}

async fn serve(
    mut socket: TcpStream,
    secret: &str,
    hits: &Mutex<Vec<Hit>>,
    status: u16,
    delay_ms: u64,
) -> std::io::Result<()> {
    let mut data: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let head = loop {
        let read = socket.read(&mut chunk).await?;
        if read == 0 {
            return Ok(());
        }
        data.extend_from_slice(&chunk[..read]);
        if let Some(at) = data.windows(4).position(|window| window == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let text = String::from_utf8_lossy(&data[..head]).to_string();
    let length = field(&text, "content-length")
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(0);
    while data.len() < head + length {
        let read = socket.read(&mut chunk).await?;
        if read == 0 {
            return Ok(());
        }
        data.extend_from_slice(&chunk[..read]);
    }
    let body = String::from_utf8_lossy(&data[head..head + length]).to_string();
    let signature = field(&text, "x-cyg-signature").unwrap_or_default();
    let timestamp = field(&text, "x-cyg-timestamp").unwrap_or_default();
    let delivery = field(&text, "x-cyg-delivery").unwrap_or_default();
    let valid = engine_api::infra::webhook::verifies(
        secret,
        &timestamp,
        &body,
        &signature,
        chrono::Utc::now().timestamp(),
    );
    if delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
    // A receiver that cannot prove the delivery came from the control plane has
    // nothing worth answering, whatever the endpoint was configured to say.
    let (code, reason) = if !valid {
        (401, "Bad Signature")
    } else if status == 200 {
        (200, "OK")
    } else {
        (status, "Server Said No")
    };
    socket
        .write_all(
            format!("HTTP/1.1 {code} {reason}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await?;
    socket.shutdown().await?;
    hits.lock().expect("the receiver log").push(Hit {
        body,
        signature,
        timestamp,
        delivery,
        valid,
    });
    Ok(())
}

/// A header value, matched case-insensitively the way HTTP requires.
fn field(head: &str, name: &str) -> Option<String> {
    head.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim().to_owned())
    })
}
