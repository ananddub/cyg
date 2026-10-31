pub fn md5_hex(raw: &str) -> String {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())
}
