pub(crate) fn ok(body: &[u8]) -> Vec<u8> {
    framed(200, "OK", body)
}

pub(crate) fn framed(status: u16, reason: &str, body: &[u8]) -> Vec<u8> {
    let mut out = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

pub(crate) fn close_delimited(body: &[u8]) -> Vec<u8> {
    let mut out = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
    out.extend_from_slice(body);
    out
}

pub(crate) fn chunked(body: &[u8]) -> Vec<u8> {
    let mut out =
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec();
    for piece in body.chunks(3) {
        out.extend_from_slice(format!("{:x}\r\n", piece.len()).as_bytes());
        out.extend_from_slice(piece);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"0\r\n\r\n");
    out
}

pub(crate) fn truncated_body() -> Vec<u8> {
    b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{\"st".to_vec()
}

pub(crate) fn garbage() -> Vec<u8> {
    b"this is not a status line\r\n\r\n".to_vec()
}

pub(crate) fn non_utf8_body() -> Vec<u8> {
    let mut body = vec![0xff, 0xfe, 0x00, b'{'];
    body.extend_from_slice(b"\"state\":\"Ready\"}");
    framed(200, "OK", &body)
}

pub(crate) fn hang_up() -> Vec<u8> {
    Vec::new()
}
