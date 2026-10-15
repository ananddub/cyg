use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// Percent-decode a URI path without allocating when there is no escape, and
/// reject a result that is not valid UTF-8 — such a request cannot name a file
/// in this repository, and answering it would mean guessing at the client's
/// intent.
pub(super) fn percent_decode(raw: &str) -> Option<Cow<'_, str>> {
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
/// Decoding happens here rather than in the extractor: `OriginalUri` hands us
/// the raw path, and a request for `%2e%2e/etc` is only dangerous once it has
/// been turned into `../etc`. Decoding is single-pass — `%252e%252e` decodes to
/// the literal name `%2e%2e`, never to `..`.
pub(super) fn child_of(base: &Path, url_path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(url_path)?;
    let mut out = base.to_path_buf();
    for seg in decoded.trim_matches('/').split('/') {
        match seg {
            "" | "." => continue,
            // Refused, never collapsed: a URL is not a filesystem path and
            // no client gets to name a file outside `base`.
            ".." => return None,
            _ => {
                // A backslash is a separator on Windows and a literal here;
                // an embedded NUL truncates the path for `open()`. Neither
                // can be a child of `base`, so both are refused outright —
                // including the `%5c` / `%00` forms the decoder just produced.
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
