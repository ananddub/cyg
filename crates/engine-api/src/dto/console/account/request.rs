//! The write bodies an operator sends for one account.

use serde::Deserialize;

/// `POST /v2/console/account/{id}/ban`. The reason is recorded in the audit trail,
/// not on the account: a ban is an operator's act, and the trail is where acts live.
#[derive(Debug, Default, Deserialize)]
pub struct BanAccountRequest {
    #[serde(default)]
    pub reason: String,
}

/// `POST /v2/console/account/{id}/note`.
#[derive(Debug, Default, Deserialize)]
pub struct AddNoteRequest {
    #[serde(default)]
    pub note: String,
}

/// `POST /v2/console/account/{id}/wallet`.
///
/// `changeset` is a map of currency to *delta*, added to what the account holds —
/// the same shape the ledger records — so an operator granting 100 gold writes
/// `{"gold": 100}` rather than the whole wallet.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateWalletRequest {
    #[serde(default)]
    pub changeset: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
}
