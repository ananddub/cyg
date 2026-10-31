use crate::infra::console::ConsoleRole;

use super::helpers::{ADMIN, PASSWORD, V2Server, login, token_for, v2_state};

#[test]
fn a_malformed_id_is_a_bad_request_naming_the_account() {
    let server = V2Server::start(v2_state());
    let auth = token_for(ConsoleRole::Admin);
    let reply = server.post("/v2/console/account/not-a-uuid/ban", Some(&auth), "{}");
    assert_eq!(
        reply.refusal(crate::error::INVALID_ARGUMENT),
        "Invalid account id."
    );
}

#[test]
fn the_status_and_route_views_answer_without_a_database() {
    let server = V2Server::start(v2_state());
    let auth = format!("Bearer {}", login(&server, ADMIN, PASSWORD));

    let reply = server.get("/v2/console/status", Some(&auth));
    assert_eq!(reply.status, 200, "{}", reply.body);
    let body = reply.json();
    assert_eq!(body["database"], false);
    assert_eq!(body["accounts"], 0);
    assert!(body["uptime_secs"].as_i64().is_some());
    assert!(body["version"].is_string());

    let reply = server.get("/v2/console/route", Some(&auth));
    assert_eq!(reply.status, 200, "{}", reply.body);
    let routes = reply.json();
    let routes = routes.as_array().expect("the explorer lists routes");
    assert!(
        routes
            .iter()
            .any(|route| route["method"] == "POST" && route["path"] == "/v2/console/authenticate"),
        "the explorer must list the console's own routes: {}",
        reply.body
    );
    assert!(
        routes.iter().all(|route| route["summary"].is_string()),
        "every route row carries a summary field"
    );
}
