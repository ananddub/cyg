use std::time::Duration;

use engine_api::infra::notify::token;
use uuid::Uuid;

use super::*;

/// The address a mail is owed to. The local part is also the lever the fake
/// relay reads: `flaky` is a relay having a bad afternoon, `bad` is a mailbox
/// that is not there.
async fn mail_player(env: &Harness, label: &str, local: &str) -> Player {
    let player = device_player(&env.client, &env.base, label).await;
    exec(
        env.db(),
        "UPDATE type::record('users', <uuid>$id) \
         SET email = $email, verify_time = time::now();",
        &[
            ("id", json!(player.uid)),
            ("email", json!(format!("{local}@notify-e2e.test"))),
        ],
    )
    .await;
    player
}

/// Somebody else's action, because a notification is written by the fact and not
/// by the account it lands on.
async fn requester(env: &Harness, label: &str) -> Player {
    device_player(&env.client, &env.base, label).await
}

async fn friend_request(env: &Harness, from: &Player, to: &Player) {
    json_body(
        env.client
            .post(format!("{}/v2/friend?ids={}", env.base, to.uid))
            .bearer_auth(&from.token)
            .send()
            .await
            .expect("friend request"),
        StatusCode::OK,
        "friend request",
    )
    .await;
}

async fn attempts(env: &Harness, uid: &str) -> Vec<Value> {
    rows(
        env.db(),
        "SELECT * FROM delivery_attempt WHERE user_id = $u \
         ORDER BY create_time ASC, id ASC",
        &[("u", json!(uid))],
    )
    .await
}

/// Waits for one of this account's messages to reach one status and answers the
/// row it reached it on.
async fn settled(env: &Harness, uid: &str, want: &str) -> Value {
    let mut seen = Vec::new();
    for _ in 0..200 {
        seen = attempts(env, uid).await;
        if let Some(row) = seen.iter().find(|row| row["status"] == want) {
            return row.clone();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!(
        "nothing owed to {uid} ever became {want}; the rows are {}",
        serde_json::to_string(&seen).unwrap_or_default()
    );
}

/// Nothing more arrives for a beat, which is how "queued nothing" is told apart
/// from "queued something slowly".
async fn quiet() {
    tokio::time::sleep(Duration::from_millis(400)).await;
}

async fn prefer(env: &Harness, player: &Player, channel: &str, kind: &str, enabled: bool) -> u16 {
    let resp = env
        .client
        .put(format!("{}/v2/notify/preferences", env.base))
        .bearer_auth(&player.token)
        .json(&json!({ "channel": channel, "kind": kind, "enabled": enabled }))
        .send()
        .await
        .expect("preference write");
    let status = resp.status().as_u16();
    if status == 200 {
        let body = resp.json::<Value>().await.expect("preference body");
        assert_eq!(body["preference"]["enabled"], json!(enabled), "{body}");
        assert_eq!(body["preference"]["kind"], json!(kind), "{body}");
    }
    status
}

fn footer_token(relay: &Relay, local: &str) -> String {
    let letter = relay
        .letter_to(local)
        .expect("the relay was offered this mail");
    let line = letter
        .lines
        .iter()
        .find(|line| line.starts_with("Stop these mails:"))
        .unwrap_or_else(|| panic!("no footer link in {letter:?}"));
    line.split("token=")
        .nth(1)
        .unwrap_or_else(|| panic!("no token on {line}"))
        .trim()
        .to_owned()
}

/// One letter, on the channel the account asked for, with the link that stops it
/// in the footer.
pub async fn one_mail_arrives(env: &Harness, relay: &Relay, ada: &Player, from: &Player) {
    let row = settled(env, &ada.uid, "delivered").await;
    assert_eq!(row["provider"], json!("smtp"), "{row}");
    assert_eq!(row["channel"], json!("email"), "{row}");
    assert_eq!(row["attempts"], json!(1), "{row}");
    assert_eq!(row["error"], json!(""), "{row}");
    assert_eq!(
        row["notification_id"].as_str().map(|s| s.len()),
        Some(36),
        "the mail has to know which in-app row it is the copy of: {row}"
    );

    let letter = relay.letter_to("ada").expect("ada's mail");
    assert!(
        letter.says(&format!(
            "Subject: {} wants to add you as a friend",
            from.username
        )),
        "the subject is not the sentence that was written: {letter:?}"
    );
    assert!(
        letter.says("From: ops@notify-e2e.test"),
        "the relay was not told who it is from: {letter:?}"
    );
    assert!(
        letter.says("Someone wants to add you."),
        "the kind's own sentence is missing: {letter:?}"
    );
    assert!(
        letter.says(&format!("{}/v2/notify/unsubscribe?token=", env.base)),
        "the footer does not lead back here: {letter:?}"
    );
    // Only mail: no sender is configured for a push, so nothing was queued for
    // one, and a queue with nothing in it is not a silent success.
    for provider in ["apns", "fcm"] {
        assert_eq!(
            count(
                env.db(),
                "delivery_attempt",
                Some("provider = $p"),
                &[("p", json!(provider))],
            )
            .await,
            0,
            "a push was queued for a provider with no credentials"
        );
    }
}

/// The link in the mail is the whole of the proof: it is opened in a mail client
/// that has never signed in here.
pub async fn the_footer_link_stops_the_channel(env: &Harness, relay: &Relay, ada: &Player) {
    let before = attempts(env, &ada.uid).await.len();
    let signed = footer_token(relay, "ada");
    let body = json_body(
        env.client
            .get(format!("{}/v2/notify/unsubscribe?token={signed}", env.base))
            .send()
            .await
            .expect("unsubscribe"),
        StatusCode::OK,
        "unsubscribe",
    )
    .await;
    assert_eq!(body["channel"], json!("email"), "{body}");

    let rows = rows(
        env.db(),
        "SELECT * FROM notification_preference WHERE user_id = $u",
        &[("u", json!(ada.uid))],
    )
    .await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["kind"], json!("*"), "{}", rows[0]);
    assert_eq!(rows[0]["enabled"], json!(false), "{}", rows[0]);

    // And from here nothing else is mailed to her, whatever the next fact is.
    let second = requester(env, "ada-two").await;
    friend_request(env, &second, ada).await;
    quiet().await;
    assert_eq!(
        attempts(env, &ada.uid).await.len(),
        before,
        "a fresh message was queued for an account that asked for it to stop"
    );
    // The in-app notification went through all the same.
    assert_eq!(
        count(
            env.db(),
            "notification",
            Some("user_id = $u"),
            &[("u", json!(ada.uid))],
        )
        .await,
        2
    );
}

pub async fn a_forged_link_opens_nothing(env: &Harness, ada: &Player) {
    let uid = uuid_of(ada);
    for (secret, ttl, what) in [
        ("a-different-secret-entirely", 3_600, "forged"),
        (SEAT_SECRET, -60, "expired"),
    ] {
        let signed = token::mint(
            secret,
            &token::Unsubscribe {
                user_id: uid,
                channel: "email".into(),
                kind: "*".into(),
            },
            ttl,
        );
        let why = refusal(
            env.client
                .get(format!("{}/v2/notify/unsubscribe?token={signed}", env.base))
                .send()
                .await
                .unwrap_or_else(|err| panic!("{what} unsubscribe link: {err}")),
            StatusCode::BAD_REQUEST,
            "refused unsubscribe",
        )
        .await;
        assert_eq!(
            why,
            "This unsubscribe link is not one this plane signed, or it has expired."
        );
    }
    let why = refusal(
        env.client
            .get(format!("{}/v2/notify/unsubscribe", env.base))
            .send()
            .await
            .expect("absent unsubscribe token"),
        StatusCode::BAD_REQUEST,
        "unsubscribe with no token",
    )
    .await;
    assert!(!why.is_empty());
}

/// Nothing is queued at all for a channel the account switched off first.
pub async fn a_switched_off_channel_queues_nothing(env: &Harness, dana: &Player) {
    assert_eq!(prefer(env, dana, "email", "*", false).await, 200);
    let from = requester(env, "dana-two").await;
    friend_request(env, &from, dana).await;
    quiet().await;
    assert!(
        attempts(env, &dana.uid).await.is_empty(),
        "a muted channel still owed a message"
    );
    assert_eq!(
        count(
            env.db(),
            "notification",
            Some("user_id = $u"),
            &[("u", json!(dana.uid))],
        )
        .await,
        1,
        "the in-app notification is not the part that was switched off"
    );
}

/// A kind's own switch is finer than the channel's, and a channel is not the
/// other channel's business.
pub async fn one_kind_does_not_switch_off_the_others(env: &Harness, bobo: &Player) {
    assert_eq!(
        prefer(env, bobo, "push", "friend_request", false).await,
        200
    );
    assert_eq!(
        prefer(env, bobo, "email", "friend_request", false).await,
        200
    );
    assert_eq!(prefer(env, bobo, "sms", "*", false).await, 400);
    let listed = json_body(
        env.client
            .get(format!("{}/v2/notify/preferences", env.base))
            .bearer_auth(&bobo.token)
            .send()
            .await
            .expect("preference list"),
        StatusCode::OK,
        "preference list",
    )
    .await;
    assert_eq!(listed["preferences"].as_array().map(|v| v.len()), Some(2));

    let from = requester(env, "bobo-two").await;
    friend_request(env, &from, bobo).await;
    quiet().await;
    assert!(
        attempts(env, &bobo.uid).await.is_empty(),
        "the kind was muted and a message went anyway"
    );
}

/// An unconfirmed address is not an address, and a barred account is not to be
/// reached at all.
pub async fn a_gate_holds_the_message_back(env: &Harness, gus: &Player, barred: &Player) {
    exec(
        env.db(),
        "UPDATE type::record('users', <uuid>$id) SET verify_time = $when;",
        &[
            ("id", json!(gus.uid)),
            ("when", json!("1970-01-01T00:00:00.000000Z")),
        ],
    )
    .await;
    let first = requester(env, "gus-two").await;
    friend_request(env, &first, gus).await;

    exec(
        env.db(),
        "UPDATE type::record('users', <uuid>$id) SET disable_time = time::now();",
        &[("id", json!(barred.uid))],
    )
    .await;
    let second = requester(env, "barred-by").await;
    friend_request(env, &second, barred).await;

    quiet().await;
    assert!(
        attempts(env, &gus.uid).await.is_empty(),
        "mail went to an address the account has not confirmed"
    );
    assert!(
        attempts(env, &barred.uid).await.is_empty(),
        "mail went to a barred account"
    );
}

/// Twice turned away, then taken: the attempts the row carries are the tries the
/// relay actually refused.
pub async fn a_busy_relay_is_owed_another_try(env: &Harness, relay: &Relay, flaky: &Player) {
    let row = settled(env, &flaky.uid, "delivered").await;
    assert_eq!(
        row["attempts"],
        json!(FLAKY_TRIES_NEEDED),
        "the letter was taken on the wrong try: {row}"
    );
    assert!(relay.letter_to("flaky").is_some(), "nothing arrived");
}

/// Twice turned away, then taken.
pub const FLAKY_TRIES_NEEDED: i64 = 3;

/// A relay that is certain needs no second attempt, and the row says who refused
/// it.
pub async fn a_refused_recipient_dies_with_the_reason(env: &Harness, bad: &Player) {
    let row = settled(env, &bad.uid, "dead").await;
    assert_eq!(row["attempts"], json!(1), "{row}");
    assert!(
        row["error"]
            .as_str()
            .unwrap_or_default()
            .contains("recipient"),
        "the row does not say who refused it: {row}"
    );
}

/// A spent budget keeps the message owed: nothing is sent, nothing is counted,
/// and the row says when it will try again.
pub async fn a_spent_budget_defers_the_message(env: &Harness, zoe: &Player) {
    // The window is written rather than spent: a fresh account has no budget row
    // yet, and an UPDATE of nothing would leave the message perfectly payable.
    exec(
        env.db(),
        "UPSERT type::record('player_quota', $id) \
         SET user_id = $u, scope = 'notify.send', used = 120, cap = 120, period_secs = 3600, \
             window_start = time::now(), update_time = time::now();",
        &[
            ("id", json!(format!("{}:notify.send", zoe.uid))),
            ("u", json!(zoe.uid.clone())),
        ],
    )
    .await;
    let from = requester(env, "zoe-two").await;
    friend_request(env, &from, zoe).await;
    quiet().await;

    let owed = rows(
        env.db(),
        "SELECT * FROM delivery_attempt WHERE user_id = $u \
           AND status = 'queued' AND next_retry_at > $horizon \
         ORDER BY create_time DESC LIMIT 1",
        &[
            ("u", json!(zoe.uid)),
            ("horizon", json!(stamp(chrono::Utc::now()))),
        ],
    )
    .await;
    assert_eq!(
        owed.len(),
        1,
        "the second message is not deferred: {owed:?}"
    );
    assert_eq!(owed[0]["attempts"], json!(0), "{}", owed[0]);
    assert_eq!(owed[0]["error"], json!(""), "{}", owed[0]);
}

/// The console's probe: an unconfigured provider is answered with the settings it
/// wants rather than a queued row nobody will ever send, and a configured one
/// answers with the row it wrote and what became of it.
pub async fn the_console_can_ask(env: &Harness, relay: &Relay, console: &str) {
    let why = refusal(
        env.client
            .post(format!("{}/v2/notify/test", env.base))
            .bearer_auth(console)
            .json(&json!({ "provider": "apns", "recipient": "deadbeef" }))
            .send()
            .await
            .expect("unconfigured test"),
        StatusCode::SERVICE_UNAVAILABLE,
        "a delivery test against a provider with no credentials",
    )
    .await;
    assert!(
        why.contains("OX_PUSH_APNS_TEAM"),
        "the refusal does not name the settings to set: {why}"
    );

    let unknown = refusal(
        env.client
            .post(format!("{}/v2/notify/test", env.base))
            .bearer_auth(console)
            .json(&json!({ "provider": "gcm", "recipient": "deadbeef" }))
            .send()
            .await
            .expect("unknown provider"),
        StatusCode::SERVICE_UNAVAILABLE,
        "a delivery test against a provider nobody built",
    )
    .await;
    assert!(unknown.contains("no sender built"), "{unknown}");

    let body = json_body(
        env.client
            .post(format!("{}/v2/notify/test", env.base))
            .bearer_auth(console)
            .json(&json!({
                "provider": "smtp",
                "recipient": "ops@notify-e2e.test",
            }))
            .send()
            .await
            .expect("mail delivery test"),
        StatusCode::OK,
        "a delivery test that was queued",
    )
    .await;
    let id = body["attempt"]["id"].as_str().expect("a queued attempt");
    assert_eq!(body["attempt"]["status"], json!("queued"), "{body}");

    let row = ask_about(env, console, id).await;
    assert_eq!(row["status"], json!("delivered"), "{row}");
    assert!(
        relay.letter_to("ops").is_some(),
        "the console asked and nothing arrived"
    );

    let gone = refusal(
        env.client
            .get(format!("{}/v2/notify/test/{}", env.base, Uuid::new_v4()))
            .bearer_auth(console)
            .send()
            .await
            .expect("an unknown attempt"),
        StatusCode::NOT_FOUND,
        "a delivery that never existed",
    )
    .await;
    assert_eq!(gone, "There is no delivery with that id.");
}

async fn ask_about(env: &Harness, console: &str, id: &str) -> Value {
    for _ in 0..200 {
        let body = json_body(
            env.client
                .get(format!("{}/v2/notify/test/{id}", env.base))
                .bearer_auth(console)
                .send()
                .await
                .expect("attempt read"),
            StatusCode::OK,
            "what became of the delivery test",
        )
        .await;
        if body["attempt"]["status"] != json!("queued") {
            return body["attempt"].clone();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the delivery test never left the queue");
}

fn uuid_of(player: &Player) -> Uuid {
    Uuid::parse_str(&player.uid).expect("a uuid account")
}

/// A time written the way the rows are written, so a `>` between two of them
/// means what it says.
fn stamp(at: chrono::DateTime<chrono::Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

/// The harness's own seat secret, which is what signs an unsubscribe link.
pub const SEAT_SECRET: &str = "e2e-seat-secret";

/// The whole surface, in the order that makes each case true on its own: a mail
/// that arrives and the link in it, the switches that stop the next one, the two
/// ways a relay can say no, and the console's probe.
pub async fn delivery_surface(env: &Harness, relay: &Relay, console: &str) {
    let ada = mail_player(env, "ada", "ada").await;
    let coco = requester(env, "coco").await;
    friend_request(env, &coco, &ada).await;
    one_mail_arrives(env, relay, &ada, &coco).await;
    a_forged_link_opens_nothing(env, &ada).await;
    the_footer_link_stops_the_channel(env, relay, &ada).await;

    let dana = mail_player(env, "dana", "dana").await;
    a_switched_off_channel_queues_nothing(env, &dana).await;

    let bobo = mail_player(env, "bobo", "bobo").await;
    one_kind_does_not_switch_off_the_others(env, &bobo).await;

    let gus = mail_player(env, "gus", "gus").await;
    let barred = mail_player(env, "hugs", "hugs").await;
    a_gate_holds_the_message_back(env, &gus, &barred).await;

    let flaky = mail_player(env, "flak", "flaky").await;
    let by = requester(env, "flaky-two").await;
    friend_request(env, &by, &flaky).await;
    a_busy_relay_is_owed_another_try(env, relay, &flaky).await;

    let bad = mail_player(env, "badd", "bad").await;
    let by = requester(env, "bad-two").await;
    friend_request(env, &by, &bad).await;
    a_refused_recipient_dies_with_the_reason(env, &bad).await;

    let zoe = mail_player(env, "zoez", "zoe").await;
    let by = requester(env, "zoe-one").await;
    friend_request(env, &by, &zoe).await;
    settled(env, &zoe.uid, "delivered").await;
    a_spent_budget_defers_the_message(env, &zoe).await;

    the_console_can_ask(env, relay, console).await;
}
