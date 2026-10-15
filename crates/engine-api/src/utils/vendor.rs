//! The vendor verification step every external provider demands, and that this
//! server cannot perform: no TLS client is linked, so a self-asserted provider
//! token would be an account-takeover hole rather than a login.
//!
//! Every route that would reach a vendor therefore runs the validation in
//! full and stops at that step with the status and message BaaS uses when the
//! call fails — the same reply a BaaS with wrong credentials gives.

use crate::error::V2Error;

/// Apple and Steam are gated by configuration *before* their credential is examined,
/// and nothing here satisfies that gate — so it is what every caller sees.
pub(crate) fn unconfigured(provider: &str) -> V2Error {
    V2Error::failed_precondition(format!("{provider} authentication is not configured."))
}

/// A token, id or signature that only the vendor could vouch for.
pub(crate) fn profile(provider: &str) -> V2Error {
    V2Error::unauthenticated(format!("Could not authenticate {provider} profile."))
}

/// The runtime-provider registry. Providers come from a game script and no script
/// runtime is hosted, so it is empty for every name — including the lowercase
/// normalisation BaaS applies here, because that is what gets stored.
pub(crate) fn no_runtime_provider(name: &str) -> V2Error {
    V2Error::not_found(format!(
        "Authentication provider not found: {}",
        name.to_lowercase()
    ))
}
