//! Which machine this process is. A room's owner is recorded by the host that
//! runs it and read by whoever has to decide whether a room may be created
//! twice, so both halves have to spell the node the same way.

/// The node's name, from the env. `OX_NODE_ID` is what a fleet sets; without it
/// the process falls back to its own id.
///
/// The fallback is the pid rather than a fixed string, and inside a container
/// the root process is pid 1 — so two pods that never set the variable do
/// collide, which is why the k8s manifests feed the GameServer's name in from
/// the downward API. Unset, it is exactly the value the seat response already
/// echoes as `processId`, so a single-node dev run recognises its own rooms with
/// no configuration at all.
pub fn node_id() -> String {
    named(
        std::env::var("OX_NODE_ID").ok().as_deref(),
        std::process::id(),
    )
}

fn named(named: Option<&str>, pid: u32) -> String {
    match named.filter(|name| !name.is_empty()) {
        Some(name) => name.to_string(),
        None => format!("cyg_{pid}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_named_node_wins_and_an_empty_name_does_not() {
        assert_eq!(named(Some("tank-3d-7f9c"), 1), "tank-3d-7f9c");
        assert_eq!(named(Some(""), 1), "cyg_1");
        assert_eq!(named(None, 4242), "cyg_4242");
    }
}
