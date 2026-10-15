//! Serving a static asset over plain HTTP through the registered hook.

use std::io::Write;

use super::super::client::WsClient;

/// Write the response and block until the body is on the wire: a static GET
/// has no event-loop follow-up, so the socket switches back to blocking mode
/// instead of leaving bytes queued for a writable event that never comes.
pub(super) fn serve_static(
    client: &mut WsClient,
    content_type: &str,
    body: &[u8],
    is_head: bool,
) -> bool {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nCache-Control: no-cache, no-store, must-revalidate\r\nPragma: no-cache\r\nExpires: 0\r\nConnection: close\r\n\r\n",
        content_type,
        body.len()
    );
    let sock = socket2::SockRef::from(&client.stream);
    let _ = sock.set_nonblocking(false);
    let _ = (&client.stream).write_all(response.as_bytes());
    if !is_head {
        let _ = (&client.stream).write_all(body);
    }
    let _ = client.stream.flush();
    client.recv_buf.clear();
    true
}
