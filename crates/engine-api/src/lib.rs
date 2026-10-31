pub mod app;
pub mod controllers;
pub mod db;
pub mod dto;
pub mod error;
pub mod infra;
pub mod moderation;
pub mod repositories;
pub mod services;
pub mod utils;

#[cfg(test)]
mod tests;

pub use app::api::router;
pub use app::serve::serve;
pub use app::state::{ApiState, OnceReceiver, auto_router, current_state, install_state};
pub use error::ApiError;
pub use infra::gc::gc_seats;
pub use infra::pump::pump;
