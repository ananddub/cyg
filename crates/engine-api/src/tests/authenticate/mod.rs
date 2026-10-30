mod device;
mod email;
mod gamecenter;
mod helpers;
mod runtime;
mod sessions;
mod social;
mod social_session;

pub(super) fn first_fields(bundle: [(&str, &str); 5], step: usize) -> String {
    bundle[..step]
        .iter()
        .map(|(name, value)| format!("\"{name}\":{value}"))
        .collect::<Vec<_>>()
        .join(",")
}
