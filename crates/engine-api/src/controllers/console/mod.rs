//! The admin console's HTTP surface (`/v2/console/*`).
//!
//! Transport only, like the player controllers: take the extractors, call the
//! service, return whatever it shapes. Everything but `authenticate` and
//! `authenticate/logout` requires a console token via
//! [`crate::app::middleware::ConsoleAuthed`].

pub mod account;
pub mod audit;
pub mod auth;
pub mod group;
pub mod iap;
pub mod leaderboard;
pub mod mfa;
pub mod notification;
pub mod setting;
pub mod status;
pub mod storage;
pub mod user;
