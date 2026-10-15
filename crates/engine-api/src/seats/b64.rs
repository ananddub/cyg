/// RFC 4648 URL-safe base64, no padding: `+`→`-`, `/`→`_`, `=` dropped.
pub fn b64url(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(T[(n >> 6 & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(T[(n & 63) as usize] as char);
        }
    }
    out
}

/// Strict unpadded b64url, matching the UDP plane's decoder byte for byte: any
/// character outside the URL-safe alphabet, any `=`, or a `len % 4 == 1`
/// remainder is not a token — it is a probe, and both planes must agree.
pub fn b64url_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }
    let bytes = input.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 == 1 || bytes.contains(&b'=') {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for group in bytes.chunks(4) {
        let mut acc = 0u32;
        let mut bits = 0;
        for &c in group {
            let v = val(c)?;
            acc = (acc << 6) | v as u32;
            bits += 6;
        }
        acc <<= 24 - bits;
        for i in 0..(bits / 8) {
            out.push((acc >> (16 - i * 8)) as u8 & 0xff);
        }
    }
    Some(out)
}
