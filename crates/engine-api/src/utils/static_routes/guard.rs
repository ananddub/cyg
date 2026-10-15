use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// Percent-decode a URI path without allocating when there is no escape, and
/// reject a non-UTF-8 result: such a request cannot name a file here.
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
        // A stray `%` is not an escape; keep it and let the lookup miss.
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

/// Map a URL path to a plain child of `base`, refusing every way out.
///
/// Decoding happens here, single-pass: `%2e%2e/etc` is only dangerous once it
/// becomes `../etc`, and `%252e%252e` decodes to the literal name `%2e%2e`, never `..`.
pub(crate) fn child_of(base: &Path, url_path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(url_path)?;
    let mut out = base.to_path_buf();
    for seg in decoded.trim_matches('/').split('/') {
        match seg {
            "" | "." => continue,
            // Refused, never collapsed: no client gets to name a file outside `base`.
            ".." => return None,
            _ => {
                // A backslash is a path separator on Windows and an embedded NUL
                // truncates for `open()`; neither can be a child of `base`.
                if seg.contains('\\') || seg.contains('\0') {
                    return None;
                }
                out.push(seg);
            }
        }
    }
    // Belt and braces: whatever the loop built must still sit inside `base`.
    out.strip_prefix(base).is_ok().then_some(out)
}
