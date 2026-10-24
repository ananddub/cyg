//! The controller layer: one module per domain.
//!
//! Each module is a `#[controller]` impl that decodes the request, calls its
//! `auto-di`-injected service and shapes the reply; the rules live in
//! [`crate::services`].

pub mod account;
pub mod auth;
pub mod channel;
pub mod console;
pub mod dashboard;
pub mod friend;
pub mod group;
pub mod health;
pub mod iap;
pub mod inventory;
pub mod leaderboard;
pub mod link;
#[path = "match.rs"]
pub mod match_routes;
pub mod matchmake;
pub mod notification;
pub mod realtime;
pub mod session;
pub mod signaling;
pub mod static_routes;
pub mod status;
pub mod storage;
pub mod storage_index;
pub mod tournament;
pub mod unlink;
pub mod user;
