//! The `/ws` wire envelope: one JSON frame per message, BaaS JSON-socket shape.
//!
//! `cid` correlates a reply with its request; `type` tags the message and the
//! payload fields sit flattened beside it, byte-identical to BaaS.

mod chat;
mod error;
mod frame;
mod matchmaker;
mod message;
mod party;
mod presence;

pub use chat::{ChannelMessage, ChannelMessageAck};
pub use error::ErrorMsg;
pub use frame::Envelope;
pub use matchmaker::{MatchmakerAdd, MatchmakerMatched, MatchmakerUser};
pub use message::EnvelopeMessage;
pub use party::Party;
pub use presence::{Presence, StatusPresence};
