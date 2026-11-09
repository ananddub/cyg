//! Server-side hooks — customer logic that runs on account events.
//!
//! The game module in `engine-wasm` is a *room*: it owns simulation state and
//! ticks. That is the wrong place for "when this player buys the VIP pack, give
//! them 1000 coins", because no room exists at that moment and the rule has
//! nothing to do with the simulation.
//!
//! A hook is the same sandbox with a different contract. It is loaded once, it
//! has no host imports, it receives one JSON document per event and answers with
//! one JSON document describing what should happen. The host applies that answer
//! through the same services the REST API uses, so a hook cannot bypass a quota,
//! a validation or an audit row — it can only propose.
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use engine_hooks::{HookEvent, HookInput, HookRunner};
//!
//! let runner = HookRunner::load(&std::fs::read("hooks.wasm")?, 64 * 1024)?;
//! let input = HookInput::new(
//!     HookEvent::Purchase,
//!     "player-1",
//!     serde_json::json!({ "sku": "vip" }),
//! );
//! if let Some(output) = runner.run(&input)? {
//!     for grant in output.grant_currency {
//!         println!("grant {} {}", grant.amount, grant.currency);
//!     }
//! }
//! # Ok(())
//! # }
//! ```

pub mod event;

pub mod runtime;
#[cfg(test)]
mod tests;

pub use event::{CurrencyGrant, HookEvent, HookInput, HookOutput, ItemGrant, Notification};
pub use runtime::{HookError, HookRunner};
