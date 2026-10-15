
pub mod history;
pub mod histogram;

pub use history::{HISTORY_LEN, DashboardHistory, HistoryEntry};
pub use histogram::{TICK_HISTOGRAM_BUCKETS, bucket_edges_us, histogram_add};
