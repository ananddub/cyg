//! The shared handler state and the process-wide access to it.

mod api_state;
mod installed;
mod once_receiver;

pub use api_state::ApiState;
pub use installed::{attach_state, auto_router, current_state, install_state, with_plane_state};
pub use once_receiver::OnceReceiver;
