//! The sim plane's status recorder: latency rings, tick windows, and the
//! once-a-second `StatusSnapshot` publish onto `bus.status_tx`.
//!
//! The web half of telemetry lives in `engine-api`, which only ever reads the
//! bus queue this module writes; nothing here knows about HTTP.

mod publish;
mod recorder;
mod report;
mod snapshot;
mod stats;

pub use publish::current;
pub use recorder::Recorder;
pub use report::PublishReport;

#[cfg(test)]
mod tests;
