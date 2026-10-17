//! `/v2/console/account` — the player accounts the console browses, and the writes
//! an operator makes on one: the ban, the note, the wallet, and the export/import
//! document that moves an account between deployments.

mod conflict;
mod export;
mod import;
mod read;
mod wallet;
mod write;

pub(super) use super::{ConsoleService, account_id, audit, id, never, pool, queries};
