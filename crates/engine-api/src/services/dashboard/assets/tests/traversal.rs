use super::*;
use crate::services::dashboard::assets;
use crate::utils::dashboard::path_guard::child_under;

#[tokio::test]
async fn no_spelling_of_a_parent_directory_escapes_the_dist_root() {
    let tree = TempDist::new("traversal");
    for probe in [
        "../../etc/passwd",
        "../../../etc/passwd",
        "..%2f..%2fetc%2fpasswd",
        "%2e%2e/%2e%2e/etc/passwd",
        "%2E%2E%2f%2E%2E%2fetc%2fpasswd",
        "..\\..\\etc\\passwd",
        "%5c%5c..%5cetc%5cpasswd",
        "/etc/passwd",
        "//etc/passwd",
        "assets/../../etc/passwd",
        "assets/%2e%2e/%2e%2e/%2e%2e/etc/passwd",
        "a%00pp.js",
        r"app.js\0",
        "%252e%252e/etc/passwd",
        "....//....//etc/passwd",
        "app.js/%25",
        "app.js/%",
    ] {
        assert_eq!(child_under(tree.dist(), probe), None, "{probe:?} was not refused");
        let (status, _, _, body) = parts(assets::serve_asset(Some(tree.dist()), probe)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{probe:?} served something");
        let text = String::from_utf8_lossy(&body);
        assert!(!text.contains("root:") && !text.contains("passwd:"), "{probe:?} leaked: {text}");
    }
}
