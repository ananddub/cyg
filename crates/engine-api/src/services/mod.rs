//! The service layer: the logic behind every controller, one module per domain.
//!
//! A service owns the rules — validation, ordering, replies — and reaches the database only through [`crate::repositories`].

pub mod account;
pub mod auth;
pub mod channel;
pub mod chat;
pub mod console;
pub mod dashboard;
pub mod friend;
pub mod group;
pub mod iap;
pub mod leaderboard;
pub mod link;
pub mod match_handler;
pub mod match_query;
pub mod matchmake;
pub mod matchmaker;
pub mod notification;
pub mod party;
pub mod presence;
pub mod session;
pub mod signaling;
pub mod social;
pub mod static_routes;
pub mod status;
pub mod storage;
pub mod tournament;
pub mod unlink;
pub mod user;
