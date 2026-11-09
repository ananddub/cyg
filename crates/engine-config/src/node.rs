pub fn node_id() -> String {
    named(
        std::env::var("CYG_NODE_ID").ok().as_deref(),
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
