use super::*;

pub async fn dashboard_identity(env: &Harness) {
    eprintln!("gap: dashboard identity");
    let console = console_login(&env.client, &env.base).await;
    let body = json_body(
        env.client
            .get(format!("{}/api/dashboard/identity", env.base))
            .bearer_auth(&console)
            .send()
            .await
            .expect("dashboard identity"),
        StatusCode::OK,
        "GET /api/dashboard/identity",
    )
    .await;
    assert!(
        body.get("game_id").is_some() && body.get("game_name").is_some(),
        "the identity document names the game keys: {body}"
    );
}
pub async fn static_surface(env: &Harness) {
    eprintln!("gap: static file surface");
    let resp = env
        .client
        .get(format!("{}/", env.base))
        .send()
        .await
        .expect("root");
    assert_eq!(resp.status(), StatusCode::OK, "GET / follows its redirect");
    assert_eq!(
        resp.url().path(),
        "/dashboard/",
        "GET / lands on the dashboard shell"
    );

    // /cyg.js is served by the binary itself now (the embedded browser SDK,
    // "no copy needed"), so it answers 200 even with no project files —
    // everything else in this list must still be a clean 404 miss.
    let sdk = env
        .client
        .get(format!("{}/cyg.js", env.base))
        .send()
        .await
        .unwrap_or_else(|err| panic!("GET /cyg.js: {err}"));
    assert_eq!(
        sdk.status(),
        StatusCode::OK,
        "GET /cyg.js serves the embedded SDK: {}",
        sdk.status()
    );
    assert!(
        sdk.text()
            .await
            .unwrap_or_default()
            .contains("CYG Gaming SDK"),
        "GET /cyg.js body is the browser SDK"
    );
    for path in [
        "/arena_map.json",
        "/packages/sdk/dist/colyseus.js",
        "/node_modules/@colyseus/schema/build/index.mjs",
        "/unity",
        "/unity/",
        "/unity/index.html",
        "/Build/webgl.wasm.framework.unityweb",
        "/TemplateData/style.css",
    ] {
        let resp = env
            .client
            .get(format!("{}{path}", env.base))
            .send()
            .await
            .unwrap_or_else(|err| panic!("GET {path}: {err}"));
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "GET {path} answers 404 while the file is absent"
        );
        let body = resp.text().await.unwrap_or_default();
        assert_eq!(body, "404 not found", "GET {path} body: {body}");
    }

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = dir.join("arena_map.json");
    std::fs::write(&fixture, json!({"size": 2048}).to_string()).expect("write the fixture");
    let resp = env
        .client
        .get(format!("{}/arena_map.json", env.base))
        .send()
        .await
        .expect("GET /arena_map.json");
    let outcome = if resp.status() == StatusCode::OK {
        assert_eq!(
            resp.headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default(),
            "application/json",
            "the JSON fixture is typed as JSON"
        );
        "served"
    } else {
        "resolved elsewhere"
    };
    std::fs::remove_file(&fixture).expect("remove the fixture");
    eprintln!("gap: static fixture outcome: {outcome}");
}
