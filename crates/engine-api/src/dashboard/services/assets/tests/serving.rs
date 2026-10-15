use super::*;
use crate::dashboard::services::assets;
use crate::dashboard::utils::path_guard::usable_dist;

#[tokio::test]
async fn a_built_tree_serves_its_shell_and_a_hashed_asset() {
    let tree = TempDist::new("serve");

    let (status, mime, cache, body) = parts(assets::serve_index(Some(tree.dist()))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mime.as_deref(), Some("text/html; charset=utf-8"));
    assert_eq!(cache.as_deref(), Some("no-store"), "a rebuilt shell must not be cached");
    assert!(body.starts_with(b"<!doctype html>"));

    let (status, mime, cache, body) = parts(assets::serve_asset(Some(tree.dist()), "assets/app-9a7c.js")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mime.as_deref(), Some("text/javascript; charset=utf-8"));
    assert_eq!(cache.as_deref(), Some("public, max-age=31536000, immutable"));
    assert_eq!(body, b"console.log(1)");

    let (status, _, _, _) = parts(assets::serve_asset(Some(tree.dist()), "assets/missing.js")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _, _) = parts(assets::serve_asset(Some(tree.dist()), "assets")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "the assets directory is not a file");
}

#[tokio::test]
async fn embedded_dashboard_serves_when_no_dist_dir_passed() {
    let (status, mime, _, body) = parts(assets::serve_index(None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mime.as_deref(), Some("text/html; charset=utf-8"));
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("<!doctype html>") || text.contains("<html"), "{text}");

    let resp = assets::serve_asset(None, "assets/definitely-missing-nonexistent-9a7c.js");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[test]
fn a_dist_without_an_index_is_not_a_dist() {
    let empty = std::env::temp_dir().join(format!(
        "openoxide-dashboard-assets-empty-{}-{}",
        std::process::id(),
        counter()
    ));
    std::fs::create_dir_all(&empty).unwrap();
    assert_eq!(usable_dist(&empty), None, "no index.html, so no dashboard");
    std::fs::write(empty.join("index.html"), b"<html>x</html>").unwrap();
    let found = usable_dist(&empty).expect("a directory holding index.html is the dist");
    assert!(found.join("index.html").is_file());
    let _ = std::fs::remove_dir_all(&empty);
}
