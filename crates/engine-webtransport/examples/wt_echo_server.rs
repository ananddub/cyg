//! Minimal WebTransport echo server on a single thread, no async runtime.
//!
//! Run with `cargo run -p engine-webtransport --example wt_echo_server`,
//! then connect from a browser (or `test-client/`-style quiche client) to
//! `https://127.0.0.1:4443/game/lobby` over WebTransport.
//!
//! The request/response builders (`WtRequest` / `WtResponse`) are re-exported
//! by the crate; you never hand-assemble header blocks at call sites.

use std::time::Duration;

use engine_webtransport::{WtConfig, WtEvent, WebTransportServer};

fn main() -> std::io::Result<()> {
    let cert = std::env::var("WT_CERT").unwrap_or_else(|_| "cert.crt".into());
    let key = std::env::var("WT_KEY").unwrap_or_else(|_| "cert.key".into());
    let bind = std::env::var("WT_BIND").unwrap_or_else(|_| "127.0.0.1:4443".into());

    let mut srv = WebTransportServer::new(WtConfig {
        bind: bind.clone(),
        cert,
        key,
        max_idle_timeout: Duration::from_secs(30),
    })?;
    println!("WebTransport (h3 draft) listening on https://{bind}");

    let mut events: Vec<WtEvent> = Vec::new();
    loop {
        srv.poll_events(Duration::from_millis(50), &mut events)?;
        for ev in events.drain(..) {
            match ev {
                WtEvent::SessionOpened { session, request } => {
                    println!(
                        "[{}] CONNECT {}{} (session-id {:?})",
                        session,
                        request.authority(),
                        request.path(),
                        request.session_id_tag()
                    );
                    srv.accept_session(session)?;
                }
                WtEvent::StreamOpened {
                    session,
                    stream,
                    dir,
                } => println!("[{session}] stream {stream} opened ({dir:?})"),
                WtEvent::StreamData {
                    session,
                    stream,
                    data,
                    fin,
                } => {
                    // Echo bytes back on the same bidi stream.
                    srv.stream_send(session, stream, &data, false)?;
                    if fin {
                        srv.stream_close(session, stream)?;
                    }
                }
                WtEvent::Datagram { session, data } => {
                    srv.send_datagram(session, &data)?;
                }
                WtEvent::SessionClosed {
                    session,
                    code,
                    reason,
                } => println!("[{session}] closed code={code:#x} reason={reason:?}"),
                WtEvent::ConnectionClosed { reason } => println!("connection closed: {reason}"),
            }
        }
    }
}
