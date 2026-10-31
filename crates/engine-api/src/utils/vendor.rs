use crate::error::V2Error;

pub(crate) fn unconfigured(provider: &str) -> V2Error {
    V2Error::failed_precondition(format!("{provider} authentication is not configured."))
}

pub(crate) fn profile(provider: &str) -> V2Error {
    V2Error::unauthenticated(format!("Could not authenticate {provider} profile."))
}

pub(crate) fn no_runtime_provider(name: &str) -> V2Error {
    V2Error::not_found(format!(
        "Authentication provider not found: {}",
        name.to_lowercase()
    ))
}
