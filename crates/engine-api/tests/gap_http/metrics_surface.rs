use super::*;

fn bearer(token: &str) -> HeaderValue {
    HeaderValue::from_str(&format!("Bearer {token}")).expect("bearer header")
}

async fn metrics(env: &Harness, auth: Option<HeaderValue>) -> (StatusCode, String) {
    let mut req = env
        .client
        .get(format!("{}/metrics", env.base))
        .header(ACCEPT, "text/plain");
    if let Some(value) = auth {
        req = req.header(AUTHORIZATION, value);
    }
    let resp = req.send().await.expect("GET /metrics");
    let status = resp.status();
    (status, resp.text().await.unwrap_or_default())
}

#[tokio::test]
async fn metrics_requires_server_key_or_console() {
    let env = Harness::start().await;

    let (status, _) = metrics(&env, None).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "/metrics must not be public"
    );

    let (status, _) = metrics(&env, Some(bearer(&Uuid::new_v4().to_string()))).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a player session must not read metrics"
    );

    let (status, body) = metrics(
        &env,
        Some(HeaderValue::from_str(&server_key_header()).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "server key reads metrics: {body}");
    assert!(
        body.contains("# TYPE cyg_"),
        "body is not Prometheus exposition: {body}"
    );

    let token = console_login(&env.client, &env.base).await;
    let (status, _) = metrics(&env, Some(bearer(&token))).await;
    assert_eq!(status, StatusCode::OK, "a console session reads metrics");

    env.stop().await;
}

#[tokio::test]
async fn metrics_expose_the_tick_histogram_and_room_phases() {
    let env = Harness::start().await;
    let (status, body) = metrics(
        &env,
        Some(HeaderValue::from_str(&server_key_header()).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for needle in [
        "# TYPE cyg_tick_hz_est gauge",
        "# TYPE cyg_tick_duration_us histogram",
        "# TYPE cyg_rooms gauge",
        "cyg_rooms{phase=\"active\"}",
        "cyg_rooms{phase=\"hibernating\"}",
        "cyg_kernel_to_sim_p50_us",
        "cyg_kernel_to_sim_p99_us",
        "cyg_tick_duration_us_bucket{le=\"+Inf\"}",
        "cyg_tick_duration_us_count",
    ] {
        assert!(body.contains(needle), "missing {needle} in:\n{body}");
    }
    env.stop().await;
}

#[tokio::test]
async fn every_response_carries_a_request_id() {
    let env = Harness::start().await;

    let resp = env
        .client
        .get(format!("{}/auto/health", env.base))
        .send()
        .await
        .expect("GET /auto/health");
    let echoed = resp
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_else(|| panic!("/auto/health response had no x-request-id"))
        .to_owned();
    assert!(
        Uuid::parse_str(&echoed).is_ok(),
        "generated id should be a uuid v4, got {echoed}"
    );

    let sent = "e2e-nested-call-7";
    let resp = env
        .client
        .get(format!("{}/auto/health", env.base))
        .header("x-request-id", sent)
        .send()
        .await
        .expect("GET /auto/health with id");
    assert_eq!(
        resp.headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok()),
        Some(sent),
        "an incoming x-request-id must be echoed"
    );

    let resp = env
        .client
        .get(format!("{}/metrics", env.base))
        .send()
        .await
        .expect("GET /metrics");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert!(
        resp.headers().contains_key("x-request-id"),
        "a 401 must still carry a request id for log correlation"
    );

    env.stop().await;
}
