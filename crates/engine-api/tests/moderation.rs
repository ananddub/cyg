use engine_api::moderation::{clean_text, is_clean, is_valid_username};

#[test]
fn test_moderation_clean_text() {
    assert!(is_clean("Hello everyone, nice game!"));
    assert!(is_clean("Let's go team blue"));
    assert!(!is_clean("fuck this game"));
    assert!(!is_clean("shit happens"));

    let sanitized = clean_text("fuck this game");
    println!("Sanitized: {}", sanitized);
    assert!(!sanitized.contains("fuck"));
    assert!(sanitized.contains("this game"));
}

#[test]
fn test_moderation_username_validation() {
    assert!(is_valid_username("PlayerOne"));
    assert!(is_valid_username("ProGamer123"));

    assert!(!is_valid_username("ab"), "Too short (< 3 chars)");
    assert!(
        !is_valid_username("a".repeat(21).as_str()),
        "Too long (> 20 chars)"
    );

    assert!(!is_valid_username("fucker99"), "Contains offensive term");
    assert!(!is_valid_username("asshole"), "Contains offensive term");
}
