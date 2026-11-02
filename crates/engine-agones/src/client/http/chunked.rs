use crate::client::MAX_BODY_BYTES;

pub(super) enum Chunk {
    Complete(Vec<u8>),
    More,
    Bad(String),
}

pub(super) fn decode_chunked(body: &[u8]) -> Chunk {
    let mut out: Vec<u8> = Vec::new();
    let mut rest = body;
    loop {
        let Some(line_end) = find_crlf(rest) else {
            return Chunk::More;
        };
        let line = match std::str::from_utf8(&rest[..line_end]) {
            Ok(line) => line,
            Err(_) => return Chunk::Bad("chunk size line was not UTF-8".into()),
        };
        let size_field = line.split(';').next().unwrap_or("").trim();
        let Ok(size) = usize::from_str_radix(size_field, 16) else {
            return Chunk::Bad(format!("chunk size {size_field:?} is not hexadecimal"));
        };
        if size == 0 {
            return Chunk::Complete(out);
        }
        let data_start = line_end + 2;
        if rest.len() < data_start + size + 2 {
            return Chunk::More;
        }
        if &rest[data_start + size..data_start + size + 2] != b"\r\n" {
            return Chunk::Bad("chunk data was not followed by CRLF".into());
        }
        out.extend_from_slice(&rest[data_start..data_start + size]);
        if out.len() > MAX_BODY_BYTES {
            return Chunk::Bad(format!(
                "chunked body exceeded the {MAX_BODY_BYTES}-byte cap"
            ));
        }
        rest = &rest[data_start + size + 2..];
    }
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}
