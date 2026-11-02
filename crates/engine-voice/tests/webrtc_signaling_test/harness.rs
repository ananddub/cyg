use bytes::Bytes;
use engine_voice::SIGNAL_VERSION;

pub fn join_body(capabilities: &str) -> Bytes {
    Bytes::from(format!(
        r#"{{"version":{SIGNAL_VERSION},"capabilities":"{capabilities}"}}"#
    ))
}
