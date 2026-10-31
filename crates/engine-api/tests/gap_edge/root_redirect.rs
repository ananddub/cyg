use super::*;

pub async fn root_redirect(env: &Harness) {
    eprintln!("gap: root redirect");
    let no_follow = reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .expect("no-follow client");
    let resp = no_follow
        .get(format!("{}/", env.base))
        .send()
        .await
        .expect("GET /");
    assert_eq!(
        resp.status(),
        StatusCode::TEMPORARY_REDIRECT,
        "GET / is a 307, not a 301: a browser re-GETs a rebuilt page"
    );
    assert_eq!(
        resp.headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/dashboard/"),
        "the redirect targets the control dashboard"
    );

    let (typed, body) = expect_text(
        env.client
            .get(format!("{}/dashboard/", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /dashboard/ (the redirect target)",
    )
    .await;
    assert!(
        typed.starts_with("text/html"),
        "the dashboard shell is HTML: {typed}"
    );
    assert!(
        body.to_ascii_lowercase().contains("<html"),
        "the shell carries a real document: {body}"
    );
}
pub async fn static_plane(env: &Harness) {
    eprintln!("gap: static file plane");
    for (label, path) in [
        ("GET /unity/index.html", "/unity/index.html"),
        ("GET /unity", "/unity"),
        ("GET /Build/game.js", "/Build/game.js"),
        ("GET /TemplateData/style.css", "/TemplateData/style.css"),
        ("GET /cyg.js", "/cyg.js"),
        ("GET /openoxide-client.js", "/openoxide-client.js"),
        ("GET /arena_map.json", "/arena_map.json"),
    ] {
        let (typed, body) = expect_text(
            env.client
                .get(format!("{}{path}", env.base))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            label,
        )
        .await;
        assert_eq!(
            body, "404 not found",
            "{label}: the handler's own miss body"
        );
        assert!(
            typed.starts_with("text/plain"),
            "{label}: the miss is plain text, not HTML: {typed}"
        );
    }
}
pub async fn dashboard_assets(env: &Harness) {
    eprintln!("gap: dashboard assets");
    let assets_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dashboard/dist/assets");
    let mut built: Vec<(String, Vec<u8>)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&assets_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if let Ok(bytes) = std::fs::read(&path) {
                        built.push((name.to_owned(), bytes));
                    }
                }
            }
        }
    }
    built.sort();
    let sample = built.into_iter().next();
    if let Some((name, disk)) = sample {
        let resp = env
            .client
            .get(format!("{}/dashboard/assets/{name}", env.base))
            .send()
            .await
            .unwrap();
        let cache = resp
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let typed = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let served = expect_bytes(
            resp,
            StatusCode::OK,
            &format!("GET /dashboard/assets/{name}"),
        )
        .await;
        assert_eq!(
            served, disk,
            "the served asset is the file the dashboard build wrote"
        );
        assert!(
            typed.starts_with(expected_mime(&name)),
            "{name}: expected {}, got {typed}",
            expected_mime(&name)
        );
        assert!(
            cache.contains("immutable"),
            "hashed dashboard assets are advertised immutable: {cache}"
        );
    } else {
        eprintln!("gap: no dashboard build under {} — serving check skipped", assets_dir.display());
    }

    let (typed, body) = expect_text(
        env.client
            .get(format!("{}/dashboard/assets/nope.svg", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "GET /dashboard/assets/{unknown}",
    )
    .await;
    assert_eq!(body, "404 not found");
    assert!(typed.starts_with("text/plain"));
}
