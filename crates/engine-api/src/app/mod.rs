//! How the crate is wired and served: the composition root ([`api`], [`state`],
//! [`serve`]) and the shared request plumbing ([`middleware`], [`params`]).

pub mod api;
pub mod middleware;
pub mod params;
pub mod serve;
pub mod state;
