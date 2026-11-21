mod api_state;
mod installed;
pub mod lifecycle;
mod once_receiver;

pub use api_state::ApiState;
pub use installed::{
    attach_state, auto_router, current_remote, current_state, install_state, resolve,
    with_plane_state,
};
pub use lifecycle::{install as install_lifecycle, manager as lifecycle_manager};
pub use once_receiver::OnceReceiver;
