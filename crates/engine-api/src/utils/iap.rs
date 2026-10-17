//! IAP helpers: receipt identity, environment naming and cursor decoding.

use base64::Engine;
use chrono::{DateTime, Utc};
use md5::{Digest, Md5};

pub(crate) fn epoch() -> DateTime<Utc> {
    DateTime::from_timestamp(0, 0).expect("epoch is a valid time")
}

pub(crate) fn hex_id(raw: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn store_of(requested: Option<i16>, default: i16) -> i16 {
    // 0 apple, 1 google, 2 huawei, 3 facebook instant, 4 samsung: the range the
    // vendors here can name, so a bogus request value lands inside it.
    requested.unwrap_or(default).clamp(0, 4)
}

pub(crate) fn environment_of(receipt: &str, store: i16) -> i16 {
    // Apple receipts are PKCS#7 and Google tokens carry no environment, none of
    // which we parse; BaaS knows from the vendor reply, so 0 (unknown) is the
    // honest answer. The one signal we do have: `test_` product ids from our games.
    let _ = (receipt, store);
    0
}

pub(crate) fn decode_offset(cursor: Option<&str>) -> i64 {
    let Some(c) = cursor else { return 0 };
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(c) else {
        return 0;
    };
    String::from_utf8_lossy(&bytes).parse().unwrap_or(0)
}
