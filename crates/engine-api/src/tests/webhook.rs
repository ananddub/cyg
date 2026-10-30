use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::repositories::models::{WebhookDelivery, WebhookEndpoint};
use crate::repositories::webhook::QUEUED;
use crate::services::webhook::deliver::{Verdict, classify, envelope, retry_delay};
use crate::services::webhook::outbox::asks_for;

fn endpoint(events: &[&str]) -> WebhookEndpoint {
    WebhookEndpoint {
        id: Uuid::nil(),
        label: "one".to_owned(),
        url: "https://hooks.test/one".to_owned(),
        secret: "0123456789abcdef".to_owned(),
        events: events.iter().map(|event| (*event).to_owned()).collect(),
        is_active: true,
        create_time: Utc::now(),
        update_time: Utc::now(),
    }
}

fn delivery(create_time: DateTime<Utc>) -> WebhookDelivery {
    WebhookDelivery {
        id: Uuid::nil(),
        endpoint_id: Uuid::nil(),
        event_id: "quest.completed:1:win-three".to_owned(),
        event: "quest.completed".to_owned(),
        payload: serde_json::json!({ "quest_key": "win-three" }),
        attempt: 0,
        status: QUEUED.to_owned(),
        response_code: 0,
        last_error: String::new(),
        next_retry_at: create_time,
        create_time,
        update_time: create_time,
    }
}

/// The step doubles and the jitter is bounded by the doubling, so a fact is
/// always owed later than it was last time — until the cap, where it rests.
#[test]
fn a_backoff_step_never_shrinks() {
    let mut previous = std::time::Duration::from_secs(0);
    for attempt in 1..=6 {
        let step = retry_delay(attempt);
        assert!(
            step > previous,
            "attempt {attempt} waited {step:?}, which is no longer than the {previous:?} before it"
        );
        previous = step;
    }
    let capped = retry_delay(6);
    assert!(
        capped <= std::time::Duration::from_secs(90),
        "a capped step drifted to {capped:?}"
    );
}

#[test]
fn only_an_endpoint_in_trouble_owes_the_fact_again() {
    for code in [200u16, 201, 204, 299] {
        assert_eq!(classify(code), Verdict::Delivered, "{code} took it");
    }
    for code in [408u16, 429, 500, 502, 503, 504] {
        assert_eq!(classify(code), Verdict::Failing, "{code} is an outage");
    }
    for code in [400u16, 401, 403, 404, 422] {
        assert_eq!(classify(code), Verdict::Rejected, "{code} is a decision");
    }
}

/// The signature covers these exact bytes, so the key order and the absence of
/// whitespace are part of the contract, not an accident of serialization.
#[test]
fn the_body_on_the_wire_is_the_documented_envelope() {
    let at = Utc::now();
    let body = envelope(&delivery(at)).to_string();
    assert!(
        body.starts_with("{\"event\":\"quest.completed\",\"event_id\":\""),
        "the envelope keys moved: {body}"
    );
    assert!(
        body.ends_with("\",\"payload\":{\"quest_key\":\"win-three\"}}"),
        "the envelope tail moved: {body}"
    );
    assert!(
        body.contains(&format!(
            "\",\"occurred_at\":\"{}",
            crate::db::time::encode(at)
        )),
        "the envelope lost its stamp: {body}"
    );
    assert!(
        !body.contains("\": "),
        "the envelope is not compact: {body}"
    );
}

#[test]
fn an_endpoint_hears_what_it_asked_for() {
    let named = endpoint(&["quest.completed"]);
    assert!(asks_for(&named, "quest.completed"));
    assert!(
        !asks_for(&named, "account.created"),
        "an endpoint was handed a fact it never asked for"
    );
    assert!(asks_for(&endpoint(&["*"]), "account.deleted"));
}
