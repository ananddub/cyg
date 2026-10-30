pub mod histogram;
pub mod history;

pub use histogram::{TICK_HISTOGRAM_BUCKETS, bucket_edges_us, histogram_add};
pub use history::{DashboardHistory, HISTORY_LEN, HistoryEntry};
