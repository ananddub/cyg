//! The `Authenticate*` and session routes, as far as they reach without a schema:
//! which message a request earns, and in what order.

mod device;
mod email;
mod gamecenter;
mod helpers;
mod runtime;
mod sessions;
mod social;

/// The first `step` fields of `bundle`, as the members of a JSON object body.
pub(super) fn first_fields(bundle: [(&str, &str); 5], step: usize) -> String {
    bundle[..step]
        .iter()
        .map(|(name, value)| format!("\"{name}\":{value}"))
        .collect::<Vec<_>>()
        .join(",")
}
