//! `If-Match` on a console resource that is edited in place: the header decides
//! whether a write lands, and the reply names the generation that won.

#[path = "harness/mod.rs"]
mod harness_shared;

use harness_shared::*;

async fn create_template(env: &Harness, console: &str, name: &str) -> Value {
    json_body(
        env.client
            .post(format!("{}/v1/console/acl/template", env.base))
            .bearer_auth(console)
            .json(&json!({
                "name": name,
                "description": "created for the precondition test",
                "acl": {"runtime": {"read": true}},
            }))
            .send()
            .await
            .expect("create a template"),
        StatusCode::OK,
        "create an ACL template",
    )
    .await
}

fn update(
    env: &Harness,
    console: &str,
    id: &str,
    name: &str,
    if_match: Option<&str>,
) -> impl std::future::Future<Output = Result<reqwest::Response, reqwest::Error>> {
    let mut request = env
        .client
        .put(format!("{}/v1/console/acl/template/{id}", env.base))
        .bearer_auth(console)
        .json(&json!({"name": name, "description": "edited", "acl": {}}));
    if let Some(tag) = if_match {
        request = request.header("If-Match", tag);
    }
    request.send()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_edit_lands_only_on_the_generation_it_named() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;
    let created = create_template(&env, &console, "precondition-A").await;
    assert_eq!(created["version"], json!(1), "a new row is generation 1");
    let id = created["id"].as_str().expect("template id").to_owned();

    // The client read generation 1, so it may write over generation 1.
    let first = update(&env, &console, &id, "precondition-A2", Some("\"1\""))
        .await
        .expect("first edit");
    assert_eq!(first.status(), StatusCode::OK);
    let tag = first
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .expect("the reply names the new generation")
        .to_owned();
    assert_eq!(tag, "\"2\"");

    // A second editor still holding generation 1 must not win.
    let stale = update(&env, &console, &id, "precondition-A3", Some("\"1\""))
        .await
        .expect("stale edit");
    assert_eq!(
        stale.status(),
        StatusCode::CONFLICT,
        "a stale generation is refused"
    );
    let body: Value = stale.json().await.expect("refusal body");
    assert_eq!(body["error"]["code"], json!(10), "ABORTED");
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("version 2")),
        "the refusal names the generation that won: {body}"
    );

    // The same client, now holding generation 2, may write again.
    let second = update(&env, &console, &id, "precondition-A4", Some(&tag))
        .await
        .expect("second edit");
    assert_eq!(second.status(), StatusCode::OK);

    env.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_client_that_does_not_say_keeps_the_unconditional_write() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;
    let created = create_template(&env, &console, "precondition-B").await;
    let id = created["id"].as_str().expect("template id").to_owned();

    let reply = update(&env, &console, &id, "precondition-B2", None)
        .await
        .expect("unconditional edit");
    assert_eq!(reply.status(), StatusCode::OK);

    // A write without a tag still advances the generation, so a later reader
    // that does send one is not stuck on a version that never moves.
    let tag = reply
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert_eq!(tag, "\"2\"");

    env.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_tag_that_is_not_a_version_is_a_bad_request() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;
    let created = create_template(&env, &console, "precondition-C").await;
    let id = created["id"].as_str().expect("template id").to_owned();

    let reply = update(
        &env,
        &console,
        &id,
        "precondition-C2",
        Some("not-a-version"),
    )
    .await
    .expect("malformed tag");
    assert_eq!(reply.status(), StatusCode::BAD_REQUEST);
    let body: Value = reply.json().await.expect("refusal body");
    assert_eq!(body["error"]["code"], json!(3), "INVALID_ARGUMENT");

    env.stop().await;
}
