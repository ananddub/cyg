//! Matchmake helpers: the method whitelist, the room-name rule and the printable form of a method name.

/// The browser client's method whitelist. Anything else is a room handler the
/// engine does not have.
pub const MATCHMAKE_METHODS: &[&str] = &[
    "joinOrCreate",
    "join",
    "joinById",
    "create",
    "rejoin",
    "reconnect",
];

/// Room names land in a URL, a log line, a JSON string and the sim plane's room
/// map. Keep them boring.
pub fn valid_room_name(room: &str) -> bool {
    (1..=64).contains(&room.len())
        && room
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Error text goes into a JSON string, and `ApiError` quotes with Rust's `Debug`,
/// which renders a control character as `\u{1}` — not valid JSON. The method name
/// arrives percent-decoded from a URL, so those bytes are dropped rather than echoed.
pub(crate) fn printable(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(64)
        .collect()
}
