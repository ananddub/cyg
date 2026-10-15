//! Realtime chat over channels (BaaS `pipeline_channel.go` parity):
//! join/leave/message/stream-data, with wire codes 0 normal, 1 update, 2 removal.

mod cleanup;
mod helpers;
mod membership;
mod message_remove;
mod message_send;
mod message_update;
mod state;
mod streams;

pub use cleanup::cleanup;
pub use membership::{join, leave};
pub use message_remove::message_remove;
pub use message_send::message_send;
pub use message_update::message_update;
pub use streams::stream_data;
