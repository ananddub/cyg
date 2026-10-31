use somnia::Key;
use uuid::Uuid;

pub trait KeyExt {
    fn as_uuid(&self) -> Option<Uuid>;
    fn as_text(&self) -> String;
}

impl KeyExt for Key {
    fn as_uuid(&self) -> Option<Uuid> {
        match self {
            Key::Uuid(uuid) => Some(*uuid),
            Key::String(text) => Uuid::parse_str(text).ok(),
            Key::Int(_) => None,
        }
    }

    fn as_text(&self) -> String {
        match self {
            Key::String(text) => text.clone(),
            Key::Uuid(uuid) => uuid.to_string(),
            Key::Int(number) => number.to_string(),
        }
    }
}

pub fn record(table: &str, key_param: &str) -> String {
    format!("type::record('{table}', ${key_param})")
}

pub fn record_uuid(table: &str, key_param: &str) -> String {
    format!("type::record('{table}', <uuid>${key_param})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_keys_back_in_model_form() {
        let uuid = Uuid::new_v4();
        assert_eq!(Key::Uuid(uuid).as_uuid(), Some(uuid));
        assert_eq!(Key::from(uuid.to_string()).as_uuid(), Some(uuid));
        assert_eq!(Key::String("weekly".into()).as_uuid(), None);
        assert_eq!(Key::String("weekly".into()).as_text(), "weekly");
        assert_eq!(Key::Uuid(uuid).as_text(), uuid.to_string());
    }

    #[test]
    fn renders_record_literals() {
        assert_eq!(record("setting", "name"), "type::record('setting', $name)");
        assert_eq!(
            record_uuid("users", "id"),
            "type::record('users', <uuid>$id)"
        );
    }
}
