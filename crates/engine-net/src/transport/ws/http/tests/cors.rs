use super::super::cors_origin;

fn allowed(origins: &[&str]) -> Vec<String> {
    origins.iter().map(|o| (*o).to_owned()).collect()
}

#[test]
fn same_origin_is_allowed_by_default() {
    let origin = "https://play.example.com";
    assert_eq!(
        cors_origin(origin, "play.example.com", &[]).as_deref(),
        Some(origin)
    );
    assert_eq!(
        cors_origin("https://evil.example.com", "play.example.com", &[]),
        None
    );
}

#[test]
fn configured_origins_are_exact_and_wildcard_is_explicit() {
    let list = allowed(&["https://studio.example.com"]);
    assert!(cors_origin("https://studio.example.com", "game:7777", &list).is_some());
    assert_eq!(
        cors_origin("https://other.example.com", "game:7777", &list),
        None
    );
    assert!(cors_origin("https://anywhere.example", "game:7777", &allowed(&["*"])).is_some());
}

#[test]
fn a_missing_origin_gets_no_cors_header() {
    assert_eq!(cors_origin("*", "game:7777", &allowed(&["*"])), None);
}
