use std::borrow::Cow;
use std::path::{Path, PathBuf};

pub(crate) fn percent_decode(raw: &str) -> Option<Cow<'_, str>> {
    if !raw.contains('%') {
        return Some(Cow::Borrowed(raw));
    }
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2]))
        {
            out.push(h << 4 | l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok().map(Cow::Owned)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn child_of(base: &Path, url_path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(url_path)?;
    let mut out = base.to_path_buf();
    for seg in decoded.trim_matches('/').split('/') {
        match seg {
            "" | "." => continue,
            ".." => return None,
            _ => {
                if seg.contains('\\') || seg.contains('\0') {
                    return None;
                }
                out.push(seg);
            }
        }
    }
    out.strip_prefix(base).is_ok().then_some(out)
}
