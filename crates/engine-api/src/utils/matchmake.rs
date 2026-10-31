pub const MATCHMAKE_METHODS: &[&str] = &[
    "joinOrCreate",
    "join",
    "joinById",
    "create",
    "rejoin",
    "reconnect",
];

pub fn valid_room_name(room: &str) -> bool {
    (1..=64).contains(&room.len())
        && room
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub(crate) fn printable(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(64)
        .collect()
}
