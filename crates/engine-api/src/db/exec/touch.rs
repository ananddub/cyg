use std::collections::BTreeSet;

use tokio::sync::broadcast;

use super::*;

const TOUCH_QUEUE: usize = 256;
const VERBS: [&str; 5] = ["CREATE", "UPDATE", "UPSERT", "DELETE", "INSERT"];
const NOT_A_TABLE: [&str; 16] = [
    "TABLE",
    "INDEX",
    "FIELD",
    "EVENT",
    "FUNCTION",
    "ANALYZER",
    "ACCESS",
    "PARAM",
    "USER",
    "NAMESPACE",
    "DATABASE",
    "TOKEN",
    "SEQUENCE",
    "ON",
    "OR",
    "FROM",
];

pub(super) fn touch_channel() -> broadcast::Sender<String> {
    broadcast::channel(TOUCH_QUEUE).0
}

impl Datastore {
    /// Tables written by this store, one name per successful write, so a live
    /// reader can wake without knowing every table in advance.
    pub fn touches(&self) -> broadcast::Receiver<String> {
        self.touches.subscribe()
    }
}

/// The tables a statement writes to, empty when it only reads. A miss here
/// costs a dashboard page one stale refresh, never correctness, so the scan is
/// deliberately forgiving: it looks for a write verb anywhere in the text.
pub(super) fn touched_tables(surql: &str) -> Vec<String> {
    let upper = surql.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    let mut tables = BTreeSet::new();
    for verb in VERBS {
        let mut from = 0;
        while let Some(at) = upper[from..].find(verb) {
            let start = from + at;
            let end = start + verb.len();
            let boundary = (start == 0 || !is_word(bytes[start - 1]))
                && bytes.get(end).is_none_or(|b| !is_word(*b));
            if boundary {
                if let Some(table) = table_after(bytes, end) {
                    tables.insert(table);
                }
            }
            from = end;
        }
    }
    tables.into_iter().collect()
}

fn table_after(bytes: &[u8], mut at: usize) -> Option<String> {
    at = skip_space(bytes, at);
    for lead in ["ONLY", "INTO", "FROM"] {
        if word_at(bytes, at, lead) {
            at = skip_space(bytes, at + lead.len());
        }
    }
    let record = b"TYPE::RECORD(";
    if bytes.get(at..).is_some_and(|rest| rest.starts_with(record)) {
        let inside = at + record.len();
        let open = bytes[inside..].iter().position(|b| *b == b'\'')? + inside;
        let close = bytes[open + 1..].iter().position(|b| *b == b'\'')? + open + 1;
        return name(&bytes[open + 1..close]);
    }
    let len = bytes[at..].iter().take_while(|b| is_word(**b)).count();
    name(&bytes[at..at + len])
}

fn name(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?.to_ascii_lowercase();
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    if NOT_A_TABLE.contains(&text.to_ascii_uppercase().as_str()) {
        return None;
    }
    Some(text)
}

fn word_at(bytes: &[u8], at: usize, word: &str) -> bool {
    bytes
        .get(at..at + word.len())
        .is_some_and(|slice| slice.eq_ignore_ascii_case(word.as_bytes()))
        && bytes.get(at + word.len()).is_none_or(|b| !is_word(*b))
}

fn skip_space(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::touched_tables;

    #[test]
    fn reads_and_schema_changes_touch_nothing() {
        for surql in [
            "SELECT * FROM users WHERE game_id = $game",
            "RETURN 1;",
            "DEFINE TABLE IF NOT EXISTS users SCHEMALESS",
            "DEFINE INDEX IF NOT EXISTS users_game ON users FIELDS game_id",
        ] {
            assert!(touched_tables(surql).is_empty(), "{surql}");
        }
    }

    #[test]
    fn every_write_shape_names_its_table() {
        for (surql, table) in [
            (
                "CREATE type::record('purchase', $id) CONTENT $data RETURN AFTER",
                "purchase",
            ),
            (
                "UPDATE type::record('users', <uuid>$id) SET username = $u",
                "users",
            ),
            (
                "UPDATE user_edge SET state = 0, update_time = $now",
                "user_edge",
            ),
            (
                "DELETE type::record('notification', <uuid>$id)",
                "notification",
            ),
            (
                "DELETE subscription WHERE product_id = $product",
                "subscription",
            ),
            ("DELETE FROM users WHERE game_id = $game", "users"),
            ("UPSERT type::record('season', $id) CONTENT $data", "season"),
            (
                "INSERT INTO message (room, body) VALUES ($room, $body)",
                "message",
            ),
        ] {
            assert_eq!(touched_tables(surql), vec![table.to_string()], "{surql}");
        }
    }

    #[test]
    fn transactions_and_guarded_writes_each_name_their_table() {
        let transaction = "BEGIN TRANSACTION; \
            UPDATE game_config_version SET is_active = false WHERE game_id = $game; \
            UPDATE type::record('game_config_version', $id) SET is_active = true; \
            COMMIT;";
        assert_eq!(
            touched_tables(transaction),
            vec!["game_config_version".to_string()]
        );

        let guarded = "IF array::len((SELECT VALUE id FROM user_device)) = 0 { \
            CREATE type::record('user_device', $device_id) CONTENT $device; \
            CREATE type::record('users', <uuid>$id) CONTENT $user; \
            RETURN true; \
        } ELSE { RETURN false; };";
        assert_eq!(
            touched_tables(guarded),
            vec!["user_device".to_string(), "users".to_string()]
        );
    }
}
