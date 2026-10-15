#![doc = include_str!("../README.md")]

pub use auto_socket_macros::{auto_socket, on};

mod auth;
mod descriptors;
mod publisher;
mod reactivity;
mod registry;
mod rooms;
mod server;
mod state;
mod subscription;

pub use auth::{LiveIdentity, set_authenticator, set_authorizer};
pub use descriptors::{
    LiveAccessDescriptor, LiveRefreshDescriptor, LiveTableDescriptor, SocketDescriptor,
    SocketRegistrar,
};
pub use publisher::{LivePublisher, LiveStrategy, PublishError};
pub use reactivity::{
    cache_live_value, cached_live_value, notify_table_changes, set_cache_invalidator,
};
pub use server::{register, register_global};
pub use subscription::{LiveSubscription, LiveSubscriptionMessage};

#[doc(hidden)]
pub mod __private {
    pub use auto_di;
    pub use inventory;
    pub use socketioxide;
}
