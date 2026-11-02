use std::time::Duration;

mod config;
mod http;
mod io;
mod reply;
mod transport;

#[cfg(test)]
mod tests;

pub use config::SidecarConfig;
pub use reply::Reply;
pub use transport::AgonesClient;

pub(crate) const MAX_HEAD_BYTES: usize = 32 * 1024;
pub(crate) const MAX_BODY_BYTES: usize = 1024 * 1024;
pub(crate) const READ_SLICE: Duration = Duration::from_millis(50);

pub(crate) fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}
