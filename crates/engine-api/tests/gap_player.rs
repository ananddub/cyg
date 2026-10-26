//! The player-facing surface no earlier suite drives end to end: the group
//! membership verbs, the leaderboard and tournament record routes, storage
//! indexes and their query, the channel message REST plane, the player IAP
//! receipts and subscriptions, and the social authenticate/link/unlink verbs.
//!
//! Every call goes through the real router against a real Postgres 16 in a
//! Testcontainer, and every assertion reads the reply the way a game client
//! does: status, exact refusal wording from the service, and payload keys.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use uuid::Uuid;

#[path = "harness/mod.rs"]
mod harness;

use harness::*;

/// One player storage write, through the route the game itself uses.
async fn put_object(env: &Harness, token: &str, collection: &str, key: &str, value: Value) {
    json_body(
        env.client
            .put(format!("{}/v2/storage", env.base))
            .bearer_auth(token)
            .json(&json!({
                "objects": [{
                    "collection": collection,
                    "key": key,
                    "value": value.to_string(),
                }]
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/storage",
    )
    .await;
}

/// A leaderboard row seeded directly: the player plane never defines boards,
/// and the console's own board routes belong to other suites.
async fn seed_board(
    env: &Harness,
    id: &str,
    category: i16,
    duration: i32,
    sort_order: i16,
    authoritative: bool,
) {
    sqlx::query(
        "INSERT INTO leaderboard (id, title, category, duration, sort_order, authoritative) \
         VALUES ($1, $1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(category)
    .bind(duration)
    .bind(sort_order)
    .bind(authoritative)
    .execute(&env.pool)
    .await
    .expect("seed a board");
}

/// The `state` (`0 superadmin, 1 admin, 2 member, 3 join-request, 4 banned`)
/// one account holds in a group, read out of the listing the route answers.
async fn member_state(env: &Harness, token: &str, group_id: &str, uid: &str) -> Option<i16> {
    let page = json_body(
        env.client
            .get(format!("{}/v2/group/{group_id}/user?limit=100", env.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/group/{groupId}/user (state lookup)",
    )
    .await;
    page["group_users"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["user"]["id"] == uid)
        .map(|row| row["state"].as_i64().expect("member state") as i16)
}

/// Whether one account shows up in the `?state=` bucket of the listing. The
/// unbucketed listing hides join-requests and bans (`state < 3`), so the banned
/// rung is only provable through its own state filter.
async fn member_in_state(env: &Harness, token: &str, group_id: &str, uid: &str, state: i16) -> bool {
    let page = json_body(
        env.client
            .get(format!(
                "{}/v2/group/{group_id}/user?state={state}&limit=100",
                env.base
            ))
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/group/{groupId}/user?state= (bucket lookup)",
    )
    .await;
    page["group_users"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|row| row["user"]["id"] == uid)
}

/// The group verbs an admin aims at one target through `?user_ids=`.
async fn members_call(env: &Harness, token: &str, path: &str, what: &str) {
    expect(
        env.client
            .post(format!("{}{}", env.base, path))
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        what,
    )
    .await;
}

/// The membership ladder, the ban, the leave rules and the user listing's paging.
async fn group_surface(env: &Harness) {
    eprintln!("gap: group membership verbs");
    let lead = device_player(&env.client, &env.base, "lead").await;
    let one = device_player(&env.client, &env.base, "one").await;
    let two = device_player(&env.client, &env.base, "two").await;
    let three = device_player(&env.client, &env.base, "three").await;
    let stray = device_player(&env.client, &env.base, "stray").await;

    let created = json_body(
        env.client
            .post(format!("{}/v2/group", env.base))
            .bearer_auth(&lead.token)
            .json(&json!({
                "name": format!("e2e ladder {}", letters_suffix()),
                "open": true,
                "max_count": 10,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create the ladder group",
    )
    .await;
    let gid = created["group"]["id"].as_str().expect("group id").to_owned();

    // POST /v2/group/{groupId}/add takes targets as a query parameter, and more
    // than one at a time when the list is comma-joined.
    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{gid}/add?user_ids={}", one.uid),
        "POST /v2/group/{groupId}/add",
    )
    .await;
    members_call(
        env,
        &lead.token,
        &format!(
            "/v2/group/{gid}/add?user_ids={},{}",
            two.uid, three.uid
        ),
        "add two more members in one call",
    )
    .await;
    assert_eq!(member_state(env, &lead.token, &gid, &one.uid).await, Some(2));
    assert_eq!(member_state(env, &lead.token, &gid, &lead.uid).await, Some(0));

    // An add with no `user_ids` is a no-op success, as in BaaS.
    expect(
        env.client
            .post(format!("{}/v2/group/{gid}/add", env.base))
            .bearer_auth(&lead.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "add with no targets is a no-op",
    )
    .await;

    // POST /v2/group/{groupId}/promote lifts one step and never past the caller;
    // POST /v2/group/{groupId}/demote walks it back down.
    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{gid}/promote?user_ids={}", one.uid),
        "POST /v2/group/{groupId}/promote",
    )
    .await;
    assert_eq!(member_state(env, &lead.token, &gid, &one.uid).await, Some(1));
    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{gid}/demote?user_ids={}", one.uid),
        "POST /v2/group/{groupId}/demote",
    )
    .await;
    assert_eq!(member_state(env, &lead.token, &gid, &one.uid).await, Some(2));

    // A plain member holds no admin authority: the group hides behind the same
    // not-found a stranger would see.
    assert_eq!(
        refusal(
            env.client
                .post(format!(
                    "{}/v2/group/{}/add?user_ids={}",
                    env.base, gid, stray.uid
                ))
                .bearer_auth(&two.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a member trying to add",
        )
        .await,
        "Group not found or permission denied."
    );

    // POST /v2/group/{groupId}/kick removes the edge; POST /v2/group/{groupId}/ban
    // rewrites it to state 4 instead.
    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{gid}/kick?user_ids={}", two.uid),
        "POST /v2/group/{groupId}/kick",
    )
    .await;
    assert_eq!(member_state(env, &lead.token, &gid, &two.uid).await, None);
    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{gid}/ban?user_ids={}", one.uid),
        "POST /v2/group/{groupId}/ban",
    )
    .await;
    // The general listing hides a ban (`state < 3`), so the banned rung is read
    // through the `?state=4` bucket.
    assert!(
        member_in_state(env, &lead.token, &gid, &one.uid, 4).await,
        "the ban landed in the banned bucket"
    );
    assert_eq!(
        member_state(env, &lead.token, &gid, &one.uid).await,
        None,
        "the unbucketed listing hides the banned member"
    );

    // A banned member re-joining is refused by name, and kicking someone of the
    // caller's own rank or above is refused outright.
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/group/{gid}/join", env.base))
                .bearer_auth(&one.token)
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "a banned player rejoining",
        )
        .await,
        "Banned from the group."
    );
    // Make `stray` an admin, then let it try the creator: rank rules bite.
    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{gid}/add?user_ids={}", stray.uid),
        "add the stray as a member",
    )
    .await;
    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{gid}/promote?user_ids={}", stray.uid),
        "promote the stray to admin",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .post(format!(
                    "{}/v2/group/{}/kick?user_ids={}",
                    env.base, gid, lead.uid
                ))
                .bearer_auth(&stray.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an admin kicking the superadmin",
        )
        .await,
        "Cannot remove a user of equal or higher role."
    );

    // POST /v2/group/{groupId}/leave: walking out is allowed for a member, and the
    // ban does not stop the leave; the last superadmin is the one holder who cannot.
    members_call(
        env,
        &one.token,
        &format!("/v2/group/{gid}/leave"),
        "POST /v2/group/{groupId}/leave",
    )
    .await;
    assert_eq!(member_state(env, &lead.token, &gid, &one.uid).await, None);
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/group/{gid}/leave", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "the last superadmin trying to leave",
        )
        .await,
        "Cannot leave group when you are the last superadmin."
    );
    // A second leave with no edge left is the group hiding its absence.
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/group/{gid}/leave", env.base))
                .bearer_auth(&one.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "leaving a group you are not in",
        )
        .await,
        "Group not found."
    );

    // GET /v2/group/{groupId}/user?cursor= pages one bucket at a time. The
    // UNFILTERED walk does not advance: the generated resume compares
    // `state > -1`, which is true for every member, so a plain limit=1 cursor
    // replay returns the same first row forever (a BaaS quirk we assert against
    // rather than pretend otherwise). The cursor test therefore rides the state
    // filter, whose SQL resumes on `position` and genuinely walks.
    let walker = json_body(
        env.client
            .post(format!("{}/v2/group", env.base))
            .bearer_auth(&lead.token)
            .json(&json!({
                "name": format!("e2e walker {}", letters_suffix()),
                "open": true,
                "max_count": 10,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create the paging group",
    )
    .await;
    let walk_gid = walker["group"]["id"].as_str().expect("group id").to_owned();
    let mut walked = Vec::new();
    for label in ["w1x", "w2x", "w3x"] {
        walked.push(device_player(&env.client, &env.base, label).await);
    }
    for m in &walked {
        members_call(
            env,
            &lead.token,
            &format!("/v2/group/{walk_gid}/add?user_ids={}", m.uid),
            "add a paging member",
        )
        .await;
    }
    let mut seen: Vec<String> = Vec::new();
    let mut cursor = String::new();
    for _ in 0..8 {
        let url = if cursor.is_empty() {
            format!("{}/v2/group/{}/user?state=2&limit=1", env.base, walk_gid)
        } else {
            format!(
                "{}/v2/group/{}/user?state=2&limit=1&cursor={}",
                env.base, walk_gid, cursor
            )
        };
        let page = json_body(
            env.client
                .get(url)
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            "GET /v2/group/{groupId}/user?state=2&limit=1&cursor= (paging)",
        )
        .await;
        for row in page["group_users"].as_array().expect("a page").clone() {
            seen.push(row["user"]["id"].as_str().expect("member id").to_owned());
        }
        cursor = page["cursor"].as_str().unwrap_or_default().to_owned();
        if cursor.is_empty() {
            break;
        }
    }
    assert_eq!(
        seen.iter().collect::<std::collections::HashSet<_>>().len(),
        seen.len(),
        "no member repeats across the state-filtered pages: {seen:?}"
    );
    assert_eq!(
        seen.len(),
        3,
        "the walk saw each of the three members once: {seen:?}"
    );
    for m in &walked {
        assert!(
            seen.contains(&m.uid),
            "the cursor walk saw every member: {seen:?}"
        );
    }
    // The state filter answers from one bucket only.
    let members = json_body(
        env.client
            .get(format!("{}/v2/group/{gid}/user?state=2&limit=10", env.base))
            .bearer_auth(&lead.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/group/{groupId}/user?state=",
    )
    .await;
    let rows = members["group_users"].as_array().expect("a page").clone();
    assert!(
        rows.iter().all(|row| row["state"] == 2) && !rows.is_empty(),
        "the state filter keeps only members: {members}"
    );
    for (label, query, status, message) in [
        ("limit=0", "limit=0", StatusCode::BAD_REQUEST, "Invalid limit - limit must be between 1 and 100."),
        ("limit=101", "limit=101", StatusCode::BAD_REQUEST, "Invalid limit - limit must be between 1 and 100."),
        ("state=5", "state=5", StatusCode::BAD_REQUEST, "Invalid state - state must be between 0 and 4."),
        ("cursor=not-a-cursor", "cursor=not-a-cursor", StatusCode::BAD_REQUEST, "Cursor is invalid."),
    ] {
        assert_eq!(
            refusal(
                env.client
                    .get(format!("{}/v2/group/{gid}/user?{query}", env.base, query = query))
                    .bearer_auth(&lead.token)
                    .send()
                    .await
                    .unwrap(),
                status,
                &format!("the user listing refuses {label}"),
            )
            .await,
            message,
            "{label}"
        );
    }
    // Without a session the whole verb set is behind the same door.
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/group/{gid}/user", env.base))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "the user listing with no session",
        )
        .await,
        "Auth token required"
    );

    // The verbs' shared id rules.
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/group/not-a-uuid/add", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an add against a malformed group id",
        )
        .await,
        "Group ID must be a valid ID."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/group/{gid}/add?user_ids=not-a-uuid", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an add aimed at a malformed user id",
        )
        .await,
        "User ID must be a valid ID."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/group/{gid}/ban?user_ids={}", env.base, Uuid::new_v4()))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a ban aimed at an account that does not exist",
        )
        .await,
        "One or more users not found."
    );
    let ghost = Uuid::new_v4().to_string();
    for (label, path) in [
        ("add", format!("/v2/group/{ghost}/add")),
        ("kick", format!("/v2/group/{ghost}/kick")),
        ("ban", format!("/v2/group/{ghost}/ban")),
    ] {
        assert_eq!(
            refusal(
                env.client
                    .post(format!("{}{}", env.base, path))
                    .bearer_auth(&lead.token)
                    .send()
                    .await
                    .unwrap(),
                StatusCode::NOT_FOUND,
                &format!("a {label} against a group that does not exist"),
            )
            .await,
            "Group not found.",
            "{label}"
        );
    }

    // `Group is full.`: the counter caps the add the same way it caps a join.
    let packed = json_body(
        env.client
            .post(format!("{}/v2/group", env.base))
            .bearer_auth(&lead.token)
            .json(&json!({
                "name": format!("e2e packed {}", letters_suffix()),
                "open": true,
                "max_count": 2,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create the two-seat group",
    )
    .await;
    let packed_id = packed["group"]["id"].as_str().expect("group id").to_owned();
    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{packed_id}/add?user_ids={}", three.uid),
        "fill the first free seat",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .post(format!(
                    "{}/v2/group/{}/add?user_ids={}",
                    env.base, packed_id, stray.uid
                ))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "adding past the cap",
        )
        .await,
        "Group is full."
    );
}

/// The record routes of a plain leaderboard: the write operators, the list and
/// the haystack, and the caller's own delete.
async fn leaderboard_surface(env: &Harness) {
    eprintln!("gap: leaderboard records, list, haystack and delete");
    let suffix = letters_suffix();
    let board = format!("e2e_gap_board_{suffix}");
    let auth_board = format!("e2e_gap_auth_{suffix}");
    let round = format!("e2e_gap_round_{suffix}");
    seed_board(env, &board, 0, 0, 1, false).await;
    seed_board(env, &auth_board, 0, 0, 1, true).await;
    // A board with a duration is a tournament, and the leaderboard routes say so.
    seed_board(env, &round, 0, 3600, 1, false).await;

    let lead = device_player(&env.client, &env.base, "lbd").await;
    let rival = device_player(&env.client, &env.base, "lbr").await;

    // POST /v2/leaderboard/{leaderboardId} — the body IS the record message.
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/leaderboard/{}", env.base, board))
                .bearer_auth(&lead.token)
                .json(&Value::Null)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a body the gateway reads as no record at all",
        )
        .await,
        "Invalid input, record score value is required."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/leaderboard/{}", env.base, board))
                .bearer_auth(&lead.token)
                .json(&json!({ "score": "not-a-number" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a score that is neither int nor numeric string",
        )
        .await,
        "Invalid input, record score value is required."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/leaderboard/{}", env.base, board))
                .bearer_auth(&lead.token)
                .json(&json!({ "score": 10, "metadata": "not json" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "metadata that is not JSON",
        )
        .await,
        "Metadata value must be JSON, if provided."
    );
    let ghost = format!("e2e_gap_ghost_{suffix}");
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/leaderboard/{ghost}", env.base))
                .bearer_auth(&lead.token)
                .json(&json!({ "score": 10 }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a record for a board that does not exist",
        )
        .await,
        "Leaderboard not found."
    );

    // The operators: the board default is BEST, and every override answers with
    // the stored record — int64s as strings, zero-valued fields omitted.
    let first = json_body(
        env.client
            .post(format!("{}/v2/leaderboard/{board}", env.base))
            .bearer_auth(&lead.token)
            .json(&json!({
                "score": 500,
                "subscore": 7,
                "metadata": "{\"a\":1}",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/leaderboard/{leaderboardId} (first write)",
    )
    .await;
    assert_eq!(first["leaderboard_id"], board);
    assert_eq!(first["owner_id"], lead.uid);
    assert_eq!(first["score"], "500", "the int64 comes back as a string");
    assert_eq!(first["subscore"], "7");
    assert_eq!(json_string(&first["metadata"]), json!({"a": 1}));
    assert_eq!(first["rank"], "1", "with ranks on, the write names the standing");

    let ignored = json_body(
        env.client
            .post(format!("{}/v2/leaderboard/{board}", env.base))
            .bearer_auth(&lead.token)
            .json(&json!({ "score": 300 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the default BEST operator ignoring a lower score",
    )
    .await;
    assert_eq!(ignored["score"], "500", "best stays best");
    for (body, wanted) in [
        (json!({ "score": 700, "operator": 2 }), "700"),
        (json!({ "score": 50, "operator": 3 }), "750"),
        (json!({ "score": 100, "operator": 4 }), "650"),
    ] {
        let written = json_body(
            env.client
                .post(format!("{}/v2/leaderboard/{board}", env.base))
                .bearer_auth(&lead.token)
                .json(&body)
                .send()
            .await
            .unwrap(),
            StatusCode::OK,
            &format!("the {:?} operator", body["operator"]),
        )
        .await;
        assert_eq!(written["score"], wanted, "{body}");
    }
    json_body(
        env.client
            .post(format!("{}/v2/leaderboard/{board}", env.base))
            .bearer_auth(&rival.token)
            .json(&json!({ "score": 400 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the rival scoring in",
    )
    .await;

    // GET /v2/leaderboard/{leaderboardId} — the page, the cap, the owners batch.
    let page = json_body(
        env.client
            .get(format!("{}/v2/leaderboard/{board}?limit=1", env.base))
            .bearer_auth(&lead.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/leaderboard/{leaderboardId}?limit=1",
    )
    .await;
    let rows = page["records"].as_array().expect("a page").clone();
    assert_eq!(rows.len(), 1, "{page}");
    assert_eq!(rows[0]["score"], "650", "the board reads in score order");
    assert_ne!(page["next_cursor"], json!(""), "a page that cut records off hands back a cursor");
    let wide = json_body(
        env.client
            .get(format!("{}/v2/leaderboard/{board}?limit=10", env.base))
            .bearer_auth(&lead.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/leaderboard/{leaderboardId} (full page)",
    )
    .await;
    assert_eq!(wide["records"].as_array().map(Vec::len), Some(2), "{wide}");
    assert_eq!(wide["rank_count"], "2");
    let owners = json_body(
        env.client
            .get(format!(
                "{}/v2/leaderboard/{board}?owner_ids={}",
                env.base,
                format!("{},{}", rival.uid, lead.uid)
            ))
            .bearer_auth(&lead.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/leaderboard/{leaderboardId}?owner_ids= (batch)",
    )
    .await;
    assert_eq!(owners["owner_records"].as_array().map(Vec::len), Some(2), "{owners}");
    assert!(owners.get("records").is_none(), "an un-paged owner batch lists no page");
    for (label, query, message) in [
        ("limit=0", "limit=0", "Invalid limit - limit must be between 1 and 1000."),
        ("limit=abc", "limit=abc", "Invalid limit - limit must be between 1 and 1000."),
        ("a bad owner id", "owner_ids=not-a-uuid", "One or more owner IDs are invalid."),
        ("a bad expiry", "expiry=soon", "Invalid expiry - expiry must be a UNIX timestamp."),
    ] {
        assert_eq!(
            refusal(
                env.client
                    .get(format!("{}/v2/leaderboard/{board}?{query}", env.base))
                    .bearer_auth(&lead.token)
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                &format!("the listing refusing {label}"),
            )
            .await,
            message,
            "{label}"
        );
    }

    // GET /v2/leaderboard/{leaderboardId}/owner/{ownerId} — the haystack: the
    // owner's record with neighbours around it.
    let hay = json_body(
        env.client
            .get(format!("{}/v2/leaderboard/{board}/owner/{}", env.base, rival.uid))
            .bearer_auth(&lead.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/leaderboard/{leaderboardId}/owner/{ownerId}",
    )
    .await;
    assert_eq!(
        hay["records"]
            .as_array()
            .map(|rows| rows.iter().any(|r| r["owner_id"] == rival.uid))
            .unwrap_or(false),
        true,
        "the haystack is anchored on the owner: {hay}"
    );
    assert_eq!(hay["rank_count"], "2");
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/leaderboard/{board}/owner/not-a-uuid", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a haystack for an account that is not a uuid",
        )
        .await,
        "Invalid owner ID provided."
    );
    // A haystack for a scoreless owner is an empty listing, not an error.
    assert_eq!(
        json_body(
            env.client
                .get(format!("{}/v2/leaderboard/{board}/owner/{}", env.base, Uuid::new_v4()))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            "a haystack for an owner with no record",
        )
        .await,
        json!({}),
        "the empty page prints nothing at all"
    );

    // DELETE /v2/leaderboard/{leaderboardId} removes the CALLER's record; the
    // rival's score survives, and the authoritative board refuses both verbs.
    expect(
        env.client
            .delete(format!("{}/v2/leaderboard/{board}", env.base))
            .bearer_auth(&lead.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/leaderboard/{leaderboardId}",
    )
    .await;
    assert_eq!(
        json_body(
            env.client
                .get(format!("{}/v2/leaderboard/{board}/owner/{}", env.base, lead.uid))
                .bearer_auth(&lead.token)
                .send()
                .await
            .unwrap(),
            StatusCode::OK,
            "the deleted owner's haystack",
        )
        .await,
        json!({}),
        "the caller's record is gone"
    );
    let survivors = json_body(
        env.client
            .get(format!("{}/v2/leaderboard/{board}?limit=10", env.base))
            .bearer_auth(&rival.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the board after the record delete",
    )
    .await;
    assert_eq!(survivors["records"].as_array().map(Vec::len), Some(1), "{survivors}");
    assert_eq!(survivors["records"][0]["owner_id"], rival.uid);
    for (label, call) in [
        (
            "writing to an authoritative board",
            env.client
                .post(format!("{}/v2/leaderboard/{auth_board}", env.base))
                .bearer_auth(&lead.token)
                .json(&json!({ "score": 10 }))
                .send()
                .await
                .unwrap(),
        ),
        (
            "deleting from an authoritative board",
            env.client
                .delete(format!("{}/v2/leaderboard/{auth_board}", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
        ),
    ] {
        refusal(call, StatusCode::FORBIDDEN, label).await;
    }
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/leaderboard/{auth_board}", env.base))
                .bearer_auth(&lead.token)
                .json(&json!({ "score": 10 }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "the authoritative write",
        )
        .await,
        "Leaderboard only allows authoritative score submissions."
    );
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/leaderboard/{auth_board}", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "the authoritative delete",
        )
        .await,
        "Leaderboard only allows authoritative score deletions."
    );
    // A duration-bearing board is a round, and the leaderboard route refuses to
    // speak for it — the client must reach for the tournament verb instead.
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/leaderboard/{round}", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "deleting a tournament record through the leaderboard route",
        )
        .await,
        "Leaderboard not found."
    );
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/leaderboard/{ghost}", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "listing a board that does not exist",
        )
        .await,
        "Leaderboard not found."
    );
}

/// The tournament record routes over boards the console defines: the join gate,
/// the score write, the round page, the entrant's standing and the delete.
async fn tournament_surface(env: &Harness, console: &str) {
    eprintln!("gap: tournament round, join, scores and standing");
    let suffix = letters_suffix();
    let joined_id = format!("e2e_gap_tjoin_{suffix}");
    let closed_id = format!("e2e_gap_tclosed_{suffix}");
    let open_id = format!("e2e_gap_topen_{suffix}");
    let plain_id = format!("e2e_gap_tplain_{suffix}");
    let now = Utc::now();
    let stamp = |offset: i64| (now + ChronoDuration::seconds(offset)).to_rfc3339();

    // joined_id asks for a join and is live; closed_id asks for a join and has
    // already ended; open_id takes scores without asking for a join and is live.
    for (id, join_required, start) in [
        (joined_id.as_str(), true, stamp(-300)),
        (closed_id.as_str(), true, stamp(-7_200)),
        (open_id.as_str(), false, stamp(-300)),
    ] {
        json_body(
            env.client
                .post(format!("{}/v2/console/tournament", env.base))
                .bearer_auth(console)
                .json(&json!({
                    "id": id,
                    "title": format!("Gap Round {id}"),
                    "duration": 3_600,
                    "start_time": start,
                    "join_required": join_required,
                }))
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            &format!("define the round {id}"),
        )
        .await;
    }
    seed_board(env, &plain_id, 0, 0, 1, false).await;

    let entrant = device_player(&env.client, &env.base, "tent").await;
    let other = device_player(&env.client, &env.base, "toth").await;

    // A write whose body names no record is refused before the board is loaded.
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/tournament/{joined_id}", env.base))
                .bearer_auth(&entrant.token)
                .json(&Value::Null)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a body that names no record",
        )
        .await,
        "Invalid input, record score value is required."
    );

    // A join-required round refuses the score until the entrant has taken a seat.
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/tournament/{joined_id}", env.base))
                .bearer_auth(&entrant.token)
                .json(&json!({ "score": 120 }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "scoring before joining",
        )
        .await,
        "Must join tournament before attempting to write value."
    );

    // A closed join-required round will not take a seat — the join verb's words.
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/tournament/{closed_id}/join", env.base))
                .bearer_auth(&entrant.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "joining a round that already closed",
        )
        .await,
        "Tournament is not active and cannot accept new joins."
    );
    // ...and will not take a score into the dead window: the write path checks the
    // active window before it reaches the join gate, so the words name the score.
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/tournament/{closed_id}", env.base))
                .bearer_auth(&entrant.token)
                .json(&json!({ "score": 10 }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "scoring into a closed round",
        )
        .await,
        "Tournament is not active and cannot accept new scores."
    );

    // POST /v2/tournament/{tournamentId}/join on a round that does not ask for
    // one is a success that writes nothing — the no-op BaaS keeps.
    expect(
        env.client
            .post(format!("{}/v2/tournament/{open_id}/join", env.base))
            .bearer_auth(&entrant.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/tournament/{tournamentId}/join on a round that does not ask for one",
    )
    .await;
    let no_op: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM leaderboard_record WHERE leaderboard_id = $1 AND owner_id = $2",
    )
    .bind(&open_id)
    .bind(Uuid::parse_str(&entrant.uid).expect("entrant uuid"))
    .fetch_one(&env.pool)
    .await
    .expect("count the no-op join rows");
    assert_eq!(no_op, 0, "the no-op join wrote nothing");

    // POST /v2/tournament/{tournamentId} — the body is the record message here too.
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/tournament/{joined_id}", env.base))
                .bearer_auth(&entrant.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a body that names no record",
        )
        .await,
        "Invalid input, record score value is required."
    );
    expect(
        env.client
            .post(format!("{}/v2/tournament/{joined_id}/join", env.base))
            .bearer_auth(&entrant.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/tournament/{tournamentId}/join",
    )
    .await;
    let wrote = json_body(
        env.client
            .post(format!("{}/v2/tournament/{joined_id}", env.base))
            .bearer_auth(&entrant.token)
            .json(&json!({ "score": 120, "metadata": "{\"lane\":\"gap\"}" }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/tournament/{tournamentId}",
    )
    .await;
    assert_eq!(wrote["leaderboard_id"], joined_id);
    assert_eq!(wrote["owner_id"], entrant.uid);
    assert_eq!(wrote["score"], "120", "the int64 comes back as a string");
    assert_eq!(json_string(&wrote["metadata"]), json!({"lane": "gap"}));
    // A round that never ends has no deadline to land in, so its records share the
    // epoch window — which is the very bucket the round listing reads.
    assert!(
        wrote.get("expiry_time").is_none(),
        "an endless round writes no deadline: {wrote}"
    );

    // A bounded round is the other half: the score is filed under the round's own
    // end, so the next round starts empty without anyone deleting anything.
    let ending_id = format!("e2e_gap_tend_{suffix}");
    json_body(
        env.client
            .post(format!("{}/v2/console/tournament", env.base))
            .bearer_auth(console)
            .json(&json!({
                "id": &ending_id,
                "title": "Gap Bounded Round",
                "duration": 3_600,
                "start_time": stamp(-300),
                "end_time": stamp(1_800),
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "define a round that ends",
    )
    .await;
    let bounded = json_body(
        env.client
            .post(format!("{}/v2/tournament/{ending_id}", env.base))
            .bearer_auth(&entrant.token)
            .json(&json!({ "score": 5 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "scoring into the round that ends",
    )
    .await;
    let deadline = DateTime::parse_from_rfc3339(
        bounded["expiry_time"]
            .as_str()
            .expect("the record names its round"),
    )
    .expect("the deadline is an RFC 3339 timestamp");
    assert_eq!(
        deadline.timestamp(),
        (now + ChronoDuration::seconds(1_800)).timestamp(),
        "the record sits in the round's own window: {bounded}"
    );

    // PUT /v2/tournament/{tournamentId} shares the write, so the operator
    // override is what distinguishes the update from the insert.
    let replaced = json_body(
        env.client
            .put(format!("{}/v2/tournament/{joined_id}", env.base))
            .bearer_auth(&entrant.token)
            .json(&json!({ "score": 90, "operator": 2 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/tournament/{tournamentId}",
    )
    .await;
    assert_eq!(replaced["score"], "90", "SET replaces the round score");

    json_body(
        env.client
            .post(format!("{}/v2/tournament/{joined_id}/join", env.base))
            .bearer_auth(&other.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the second entrant joining",
    )
    .await;
    json_body(
        env.client
            .post(format!("{}/v2/tournament/{joined_id}", env.base))
            .bearer_auth(&other.token)
            .json(&json!({ "score": 130 }))
            .send()
        .await
        .unwrap(),
        StatusCode::OK,
        "the second entrant scoring",
    )
    .await;

    // GET /v2/tournament/{tournamentId} — the round page.
    let page = json_body(
        env.client
            .get(format!("{}/v2/tournament/{joined_id}?limit=5", env.base))
            .bearer_auth(&entrant.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/tournament/{tournamentId}",
    )
    .await;
    let rows = page["records"].as_array().expect("a page").clone();
    assert_eq!(rows.len(), 2, "{page}");
    assert_eq!(rows[0]["score"], "130", "the round reads in score order");
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/tournament/{joined_id}?limit=0", env.base))
                .bearer_auth(&entrant.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "the round page refusing limit=0",
        )
        .await,
        "Invalid limit - limit must be between 1 and 100."
    );

    // GET /v2/tournament/{tournamentId}/owner/{ownerId} — the entrant's standing.
    let standing = json_body(
        env.client
            .get(format!("{}/v2/tournament/{joined_id}/owner/{}", env.base, other.uid))
            .bearer_auth(&entrant.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/tournament/{tournamentId}/owner/{ownerId}",
    )
    .await;
    assert_eq!(
        standing["records"]
            .as_array()
            .map(|rows| rows.iter().any(|r| r["owner_id"] == other.uid))
            .unwrap_or(false),
        true,
        "the haystack anchors on the entrant: {standing}"
    );
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/tournament/{joined_id}/owner/not-a-uuid", env.base))
                .bearer_auth(&entrant.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a standing for an account that is not a uuid",
        )
        .await,
        "Invalid owner ID provided."
    );

    // DELETE /v2/tournament/{tournamentId} drops the caller's record for the round.
    expect(
        env.client
            .delete(format!("{}/v2/tournament/{joined_id}", env.base))
            .bearer_auth(&other.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/tournament/{tournamentId}",
    )
    .await;
    let after = json_body(
        env.client
            .get(format!("{}/v2/tournament/{joined_id}?limit=5", env.base))
            .bearer_auth(&entrant.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the round after the delete",
    )
    .await;
    assert_eq!(after["records"].as_array().map(Vec::len), Some(1), "{after}");
    assert_eq!(after["records"][0]["owner_id"], entrant.uid);

    // The board-kind rules: plain boards and absent ids answer
    // `Tournament not found.`, while a delete of a board that is not there at all
    // is the internal error BaaS restates (the switch only maps found rows).
    for (label, id, status, message) in [
        ("listing a plain board as a round", &plain_id, StatusCode::NOT_FOUND, "Tournament not found."),
        ("writing to a round that does not exist", &format!("e2e_gap_missing_{suffix}"), StatusCode::NOT_FOUND, "Tournament not found."),
    ] {
        assert_eq!(
            refusal(
                env.client
                    .get(format!("{}/v2/tournament/{id}", env.base))
                    .bearer_auth(&entrant.token)
                    .send()
                    .await
                    .unwrap(),
                status,
                label,
            )
            .await,
            message,
            "{label}"
        );
    }
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/tournament/{plain_id}", env.base))
                .bearer_auth(&entrant.token)
                .json(&json!({ "score": 10 }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "scoring into a plain leaderboard",
        )
        .await,
        "Tournament not found."
    );
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/tournament/{plain_id}", env.base))
                .bearer_auth(&entrant.token)
                .send()
                .await
                .unwrap(),
            // The route's own comment says it: an absent or non-tournament row is
            // the delete's internal failure here, restated exactly as BaaS does.
            StatusCode::INTERNAL_SERVER_ERROR,
            "deleting from a plain leaderboard through the tournament route",
        )
        .await,
        "Error deleting score from tournament."
    );
}

/// The storage-index plane: the definition table, its owner-only delete, and the
/// indexed query — including its paging cursor.
async fn storage_index_surface(env: &Harness) {
    eprintln!("gap: storage index and query");
    let suffix = letters_suffix();
    let owner = device_player(&env.client, &env.base, "sidx").await;
    let stranger = device_player(&env.client, &env.base, "sid2").await;
    let collection = format!("e2e_gap_idx_{suffix}");
    let index = format!("e2e_gap_index_{suffix}");

    for (key, hp) in [("obj-a", 100u32), ("obj-b", 200), ("obj-c", 300)] {
        put_object(env, &owner.token, &collection, key, json!({ "hp": hp })).await;
    }

    // The validation ladder the runtime applies before it registers a name.
    for (label, body, message) in [
        ("no name", json!({"collection": collection, "fields": ["hp"]}), "Storage index name must be set."),
        ("no collection", json!({"name": index, "fields": ["hp"]}), "Storage index collection must be set."),
        ("no fields", json!({"name": index, "collection": collection}), "Storage index fields must name at least one top-level key of the object value."),
        ("five fields", json!({"name": index, "collection": collection, "fields": ["a","b","c","d","e"]}), "Storage index fields must name at most four top-level keys."),
        ("zero max entries", json!({"name": index, "collection": collection, "fields": ["hp"], "maxEntries": 0}), "Storage index max entries must be greater than zero."),
    ] {
        assert_eq!(
            refusal(
                env.client
                    .post(format!("{}/v2/storage/index", env.base))
                    .bearer_auth(&owner.token)
                    .json(&body)
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                &format!("an index definition with {label}"),
            )
            .await,
            message,
            "{label}"
        );
    }
    let defined = json_body(
        env.client
            .post(format!("{}/v2/storage/index", env.base))
            .bearer_auth(&owner.token)
            .json(&json!({
                "name": index,
                "collection": collection,
                "key": "",
                "fields": ["hp"],
                "sortableFields": ["hp"],
                "maxEntries": 100,
                "indexOnly": false,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/storage/index",
    )
    .await;
    assert_eq!(defined["index"], index, "the definition names itself under `index`");
    assert_eq!(defined["collection"], collection);
    assert_eq!(defined["fields"], json!(["hp"]));
    assert_eq!(defined["max_entries"], 100);
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/storage/index", env.base))
                .bearer_auth(&owner.token)
                .json(&json!({ "name": index, "collection": collection, "fields": ["hp"] }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "clashing with the one shared namespace",
        )
        .await,
        "Cannot create index: an index with this name already exists."
    );

    let listed = json_body(
        env.client
            .get(format!("{}/v2/storage/index", env.base))
            .bearer_auth(&owner.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/storage/index",
    )
    .await;
    assert!(
        listed["indexes"]
            .as_array()
            .map(|rows| rows.iter().any(|row| row["index"] == index))
            .unwrap_or(false),
        "the definition is listed: {listed}"
    );

    // POST /v2/storage/index/{indexId}/query — equality, range, paging, order.
    let url = |env: &Harness, name: &str| format!("{}/v2/storage/index/{name}/query", env.base);
    let matched = json_body(
        env.client
            .post(url(env, &index))
            .bearer_auth(&owner.token)
            .json(&json!({ "query": "hp:200", "limit": 10 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/storage/index/{indexId}/query (equality)",
    )
    .await;
    assert_eq!(matched["objects"].as_array().map(Vec::len), Some(1), "{matched}");
    assert_eq!(json_string(&matched["objects"][0]["value"]), json!({"hp": 200}));
    assert_eq!(matched["objects"][0]["key"], "obj-b");
    let ranged = json_body(
        env.client
            .post(url(env, &index))
            .bearer_auth(&owner.token)
            .json(&json!({ "query": "hp:[100 TO 300]", "limit": 10, "order": ["-hp"] }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/storage/index/{indexId}/query (range, sorted)",
    )
    .await;
    let rows = ranged["objects"].as_array().expect("a page").clone();
    assert_eq!(rows.len(), 3, "{ranged}");
    assert_eq!(rows[0]["key"], "obj-c", "the '-' prefix sorts descending");

    // An `OR` is not in the small grammar: the tokens `hp:100`, `OR`, `hp:200`
    // are ANDed and the bare `OR` has no colon, so it fails to parse.
    assert_eq!(
        refusal(
            env.client
                .post(url(env, &index))
                .bearer_auth(&owner.token)
                .json(&json!({ "query": "hp:100 OR hp:200", "limit": 10 }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an OR term the grammar does not speak",
        )
        .await,
        "Failed to parse the index query."
    );

    // The cursor replays the same search: walk hp:[100 TO 300] one row at a time.
    let mut keys: Vec<String> = Vec::new();
    let mut cursor = String::new();
    let mut first_cursor = String::new();
    for _ in 0..5 {
        let page = json_body(
            env.client
                .post(url(env, &index))
                .bearer_auth(&owner.token)
                .json(&json!({ "query": "hp:[100 TO 300]", "limit": 1, "order": ["hp"], "cursor": cursor }))
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            "POST /v2/storage/index/{indexId}/query (cursor paging)",
        )
        .await;
        for row in page["objects"].as_array().expect("a page").clone() {
            keys.push(row["key"].as_str().expect("key").to_owned());
        }
        cursor = page["next_cursor"].as_str().unwrap_or_default().to_owned();
        if first_cursor.is_empty() {
            first_cursor = cursor.clone();
        }
        if cursor.is_empty() {
            break;
        }
    }
    assert_eq!(keys, vec!["obj-a", "obj-b", "obj-c"], "the walk saw each object once");
    // A cursor only replays for the exact query, limit and order that made it;
    // drifting any of them is refused like a malformed one.
    assert_eq!(
        refusal(
            env.client
                .post(url(env, &index))
                .bearer_auth(&owner.token)
                .json(&json!({ "query": "hp:[100 TO 200]", "limit": 1, "order": ["hp"], "cursor": first_cursor }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "replaying a cursor with a drifted query",
        )
        .await,
        "Malformed cursor was used."
    );

    for (label, body, status, message) in [
        (
            "a field the index does not cover",
            json!({ "query": "mp:5", "limit": 10 }),
            StatusCode::BAD_REQUEST,
            "This index does not cover the field named in the query.",
        ),
        (
            "limit=0",
            json!({ "query": "hp:100", "limit": 0 }),
            StatusCode::BAD_REQUEST,
            "Invalid limit - limit must be between 1 and 100.",
        ),
        (
            "a term with no colon",
            json!({ "query": "justaword", "limit": 10 }),
            StatusCode::BAD_REQUEST,
            "Failed to parse the index query.",
        ),
        (
            "an order field that is neither a column nor sortable",
            json!({ "query": "hp:100", "limit": 10, "order": ["nope"] }),
            StatusCode::BAD_REQUEST,
            "This index cannot sort results by the field named in the order.",
        ),
    ] {
        assert_eq!(
            refusal(
                env.client
                    .post(url(env, &index))
                    .bearer_auth(&owner.token)
                    .json(&body)
                    .send()
                    .await
                    .unwrap(),
                status,
                &format!("the query refusing {label}"),
            )
            .await,
            message,
            "{label}"
        );
    }
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/storage/index/e2e_missing_{suffix}/query", env.base))
                .bearer_auth(&owner.token)
                .json(&json!({ "query": "hp:100", "limit": 10 }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "querying an index that was never registered",
        )
        .await,
        "Index not found."
    );

    // DELETE /v2/storage/index/{indexId}: only the registrant may remove it.
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/storage/index/{index}", env.base))
                .bearer_auth(&stranger.token)
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "a stranger trying to drop the definition",
        )
        .await,
        "Only the player who created this index can delete it."
    );
    expect(
        env.client
            .delete(format!("{}/v2/storage/index/{index}", env.base))
            .bearer_auth(&owner.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/storage/index/{indexId}",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/storage/index/{index}", env.base))
                .bearer_auth(&owner.token)
                .send()
                .await
            .unwrap(),
            StatusCode::NOT_FOUND,
            "deleting the same definition twice",
        )
        .await,
        "Index not found."
    );
}

/// The channel REST plane: room, group and DM streams, the member gate, and the
/// owner-only message edit and tombstone.
async fn channel_surface(env: &Harness) {
    eprintln!("gap: channel messages");
    let author = device_player(&env.client, &env.base, "chat").await;
    let mate = device_player(&env.client, &env.base, "chum").await;
    let outsider = device_player(&env.client, &env.base, "outr").await;

    // A malformed id is refused before any membership question is asked.
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/channel/2", env.base))
                .bearer_auth(&author.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a channel id that is not four dotted fields",
        )
        .await,
        "Invalid channel ID."
    );

    // The room stream (mode 2) is open: anyone with a session may write and read.
    let room = format!("2...e2e-gap-room-{}", letters_suffix());
    let empty = json_body(
        env.client
            .get(format!("{}/v2/channel/{room}", env.base))
            .bearer_auth(&author.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/channel/{channelId} (empty room)",
    )
    .await;
    assert_eq!(empty["messages"].as_array().map(Vec::len), Some(0), "{empty}");
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/channel/{room}/message", env.base))
                .bearer_auth(&author.token)
                .json(&json!({ "content": "hello" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "content that is neither object nor array",
        )
        .await,
        "Message content must be a JSON object."
    );
    let sent = json_body(
        env.client
            .post(format!("{}/v2/channel/{room}/message", env.base))
            .bearer_auth(&author.token)
            .json(&json!({ "content": { "text": "gap first line" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/channel/{channelId}/message",
    )
    .await;
    assert_eq!(sent["channel_id"], room);
    assert_eq!(sent["sender_id"], author.uid);
    assert_eq!(sent["username"], author.username);
    assert_eq!(json_string(&sent["content"]), json!({ "text": "gap first line" }));
    assert_eq!(sent["room_name"], room.strip_prefix("2...").unwrap());
    let message_id = sent["message_id"].as_str().expect("message id").to_owned();
    json_body(
        env.client
            .post(format!("{}/v2/channel/{room}/message", env.base))
            .bearer_auth(&author.token)
            .json(&json!({ "content": { "text": "gap second line" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a second room message",
    )
    .await;

    // The default page is one row; `limit` walks up to 100, and `forward=false`
    // reads back from the newest.
    let head = json_body(
        env.client
            .get(format!("{}/v2/channel/{room}?limit=10", env.base))
            .bearer_auth(&author.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/channel/{channelId}?limit=10",
    )
    .await;
    let rows = head["messages"].as_array().expect("a page").clone();
    assert_eq!(rows.len(), 2, "{head}");
    assert_eq!(
        json_string(&rows[0]["content"]),
        json!({ "text": "gap first line" }),
        "forward reads oldest first"
    );
    assert!(head["cacheable_cursor"].is_string());
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/channel/{room}?limit=0", env.base))
                .bearer_auth(&author.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "the log refusing limit=0",
        )
        .await,
        "Invalid limit - limit must be between 1 and 100."
    );
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/channel/{room}?cursor=not-a-cursor", env.base))
                .bearer_auth(&author.token)
                .send()
                .await
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "the log refusing a malformed cursor",
        )
        .await,
        "Cursor is invalid or expired."
    );

    // PUT /v2/channel/{channelId}/message/{messageId} edits the content — and only
    // the caller's own messages: the update is keyed by sender.
    let edited = json_body(
        env.client
            .put(format!("{}/v2/channel/{room}/message/{message_id}", env.base))
            .bearer_auth(&author.token)
            .json(&json!({ "content": { "text": "gap edited" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/channel/{channelId}/message/{messageId}",
    )
    .await;
    assert_eq!(json_string(&edited["content"]), json!({ "text": "gap edited" }));
    assert_eq!(
        refusal(
            env.client
                .put(format!("{}/v2/channel/{room}/message/{message_id}", env.base))
                .bearer_auth(&mate.token)
                .json(&json!({ "content": { "text": "not yours" } }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "editing somebody else's message",
        )
        .await,
        "Channel message not found."
    );
    assert_eq!(
        refusal(
            env.client
                .put(format!("{}/v2/channel/{room}/message/not-a-uuid", env.base))
                .bearer_auth(&author.token)
                .json(&json!({ "content": { "text": "?" } }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an edit aimed at a malformed message id",
        )
        .await,
        "Invalid message ID."
    );

    // DELETE /v2/channel/{channelId}/message/{messageId} leaves a tombstone: code 2
    // and an emptied body, so readers who hold the message can drop it.
    let tomb = json_body(
        env.client
            .delete(format!("{}/v2/channel/{room}/message/{message_id}", env.base))
            .bearer_auth(&author.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/channel/{channelId}/message/{messageId}",
    )
    .await;
    assert_eq!(tomb["code"], 2, "the removal is a tombstone, not a vanishing");
    assert_eq!(tomb["content"], "{}");
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/channel/{room}/message/{message_id}", env.base))
                .bearer_auth(&mate.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "removing somebody else's message",
        )
        .await,
        "Channel message not found."
    );

    // The group stream (mode 3) keeps non-members out with the group's own words.
    let created = json_body(
        env.client
            .post(format!("{}/v2/group", env.base))
            .bearer_auth(&author.token)
            .json(&json!({
                "name": format!("e2e talkie {}", letters_suffix()),
                "open": true,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create the talking group",
    )
    .await;
    let gid = created["group"]["id"].as_str().expect("group id").to_owned();
    let group_channel = format!("3.{gid}..");
    json_body(
        env.client
            .post(format!("{}/v2/channel/{group_channel}/message", env.base))
            .bearer_auth(&author.token)
            .json(&json!({ "content": { "text": "group hello" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/channel/{channelId}/message (group stream)",
    )
    .await;
    expect(
        env.client
            .post(format!("{}/v2/group/{gid}/join", env.base))
            .bearer_auth(&mate.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the mate joining the group",
    )
    .await;
    let log = json_body(
        env.client
            .get(format!("{}/v2/channel/{group_channel}?limit=5", env.base))
            .bearer_auth(&mate.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/channel/{channelId} (group stream, member)",
    )
    .await;
    assert_eq!(log["messages"].as_array().map(Vec::len), Some(1), "{log}");
    assert_eq!(log["messages"][0]["group_id"], gid);
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/channel/{group_channel}", env.base))
                .bearer_auth(&outsider.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a non-member reading the group stream",
        )
        .await,
        "Group not found."
    );

    // The DM stream (mode 4) stores the pair in canonical order — the larger id
    // second — and one participant writing is the other's inbox.
    let (small, large) = if author.uid < mate.uid {
        (author.uid.clone(), mate.uid.clone())
    } else {
        (mate.uid.clone(), author.uid.clone())
    };
    let dm = format!("4.{small}.{large}.");
    json_body(
        env.client
            .post(format!("{}/v2/channel/{dm}/message", env.base))
            .bearer_auth(&author.token)
            .json(&json!({ "content": { "text": "dm ping" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a DM from one half of the pair",
    )
    .await;
    let box_ = json_body(
        env.client
            .get(format!("{}/v2/channel/{dm}?limit=5", env.base))
            .bearer_auth(&mate.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/channel/{channelId} (DM stream)",
    )
    .await;
    assert_eq!(box_["messages"].as_array().map(Vec::len), Some(1), "{box_}");
    assert_eq!(box_["messages"][0]["user_id_one"], small);
    assert_eq!(box_["messages"][0]["user_id_two"], large);
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/channel/{dm}", env.base))
                .bearer_auth(&outsider.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a stranger reading somebody's DM",
        )
        .await,
        "Channel not found."
    );
    // The same pair spelled the wrong way round is not a channel id at all.
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/channel/4.{large}.{small}.", env.base))
                .bearer_auth(&author.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "the DM ids out of canonical order",
        )
        .await,
        "Invalid channel ID."
    );
}

/// The player IAP plane: purchases and subscriptions recorded from fake-but-
/// well-formed vendor credentials, the two listings, and the per-product read.
async fn iap_surface(env: &Harness) {
    eprintln!("gap: player iap purchases and subscriptions");
    let buyer = device_player(&env.client, &env.base, "buyer").await;
    let post = |path: &str| env.client.post(format!("{}{}", env.base, path)).bearer_auth(&buyer.token);

    assert_eq!(
        refusal(
            post("/v2/iap/purchase/apple")
                .json(&json!({ "receipt": "" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an apple purchase with no receipt",
        )
        .await,
        "receipt cannot be empty"
    );
    assert_eq!(
        refusal(
            post("/v2/iap/purchase/google")
                .json(&json!({ "purchase_token": "" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a google purchase with no token",
        )
        .await,
        "purchase token cannot be empty"
    );
    assert_eq!(
        refusal(
            post("/v2/iap/subscription/apple")
                .json(&json!({ "product_id": "season.none" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a subscription with neither credential",
        )
        .await,
        "receipt or purchase token is required"
    );

    // Validation is deterministic and makes no outbound call: a well-formed fake
    // receipt is stored, its md5 digest standing in for the vendor transaction id.
    let apple = json_body(
        post("/v2/iap/purchase/apple")
            .json(&json!({ "receipt": "e2e-gap-apple-receipt", "product_id": "gem.small" }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/iap/purchase/apple",
    )
    .await;
    assert_eq!(apple["product_id"], "gem.small");
    assert_eq!(apple["transaction_id"].as_str().map(str::len), Some(32), "{apple}");
    assert_eq!(apple["environment"], "UNKNOWN", "nothing here can name the vendor environment");
    assert!(apple.get("refund_time").is_none(), "a fresh purchase has no refund");
    for (label, path, body) in [
        (
            "POST /v2/iap/purchase/google",
            "/v2/iap/purchase/google",
            json!({ "purchase_token": "e2e-gap-google-token", "product_id": "gem.large" }),
        ),
        (
            "POST /v2/iap/purchase/huawei",
            "/v2/iap/purchase/huawei",
            json!({ "purchase_token": "e2e-gap-huawei-token", "product_id": "gem.xl" }),
        ),
        (
            "POST /v2/iap/purchase/facebookinstant",
            "/v2/iap/purchase/facebookinstant",
            json!({ "signed_request": "e2e-gap-fi-token", "product_id": "gem.fb" }),
        ),
        (
            "POST /v2/iap/purchase/samsung",
            "/v2/iap/purchase/samsung",
            json!({ "purchase_id": "e2e-gap-samsung-id", "product_id": "gem.sam" }),
        ),
    ] {
        let stored = json_body(
            post(path).json(&body).send().await.unwrap(),
            StatusCode::OK,
            label,
        )
        .await;
        assert_eq!(
            stored["transaction_id"].as_str().map(str::len),
            Some(32),
            "{label} hashes its credential"
        );
    }

    for (label, path, body) in [
        (
            "POST /v2/iap/subscription/apple",
            "/v2/iap/subscription/apple",
            json!({
                "receipt": "e2e-gap-sub-receipt",
                "product_id": "season.gold",
                "expire_time": (Utc::now() + ChronoDuration::days(1)).to_rfc3339(),
            }),
        ),
        (
            "POST /v2/iap/subscription/google",
            "/v2/iap/subscription/google",
            json!({ "purchase_token": "e2e-gap-sub-token", "product_id": "season.silver" }),
        ),
        (
            "POST /v2/iap/subscription/huawei",
            "/v2/iap/subscription/huawei",
            json!({ "receipt": "e2e-gap-hw-sub", "product_id": "season.bronze" }),
        ),
    ] {
        let active = json_body(
            post(path).json(&body).send().await.unwrap(),
            StatusCode::OK,
            label,
        )
        .await;
        assert_eq!(active["state"], 1, "no expiry and no refund is an active {label}");
    }

    let purchases = json_body(
        env.client
            .get(format!("{}/v2/iap/purchase?limit=100", env.base))
            .bearer_auth(&buyer.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/iap/purchase",
    )
    .await;
    assert_eq!(purchases["purchases"].as_array().map(Vec::len), Some(5), "{purchases}");

    // The listing is BaaS's still-valid view: `expire_time > now() OR refund_time >
    // epoch`. A vendor record that named no expiry is stored in the epoch bucket, so
    // it is active but not listed — it is addressed by product id, below.
    let subscriptions = json_body(
        env.client
            .get(format!("{}/v2/iap/subscription?limit=10", env.base))
            .bearer_auth(&buyer.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/iap/subscription",
    )
    .await;
    let held = subscriptions["subscriptions"]
        .as_array()
        .expect("an array of subscriptions")
        .clone();
    assert_eq!(held.len(), 1, "{subscriptions}");
    assert_eq!(held[0]["product_id"], "season.gold");
    assert!(
        held[0]["expiration_time"].as_i64().is_some(),
        "the listed subscription names its expiry: {held:?}"
    );

    // GET /v2/iap/subscription/{productId} — the caller's newest row for one product.
    let gold = json_body(
        env.client
            .get(format!("{}/v2/iap/subscription/season.gold", env.base))
            .bearer_auth(&buyer.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/iap/subscription/{productId}",
    )
    .await;
    assert_eq!(gold["product_id"], "season.gold");
    assert_eq!(gold["state"], 1);
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/iap/subscription/season.none", env.base))
                .bearer_auth(&buyer.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a product the buyer holds nothing for",
        )
        .await,
        "Subscription not found."
    );
}

/// The five vendor authenticate verbs, each stopping at the point this build
/// cannot cross — and the server key that gates the whole plane.
async fn provider_auth_surface(env: &Harness) {
    eprintln!("gap: social authenticate verbs");
    let key = server_key_header();
    let url = |suffix: &str| format!("{}/v2/account/authenticate{suffix}", env.base);

    for (label, path) in [
        ("POST /v2/account/authenticate/apple", "/apple"),
        ("POST /v2/account/authenticate/facebook", "/facebook"),
        ("POST /v2/account/authenticate/facebookinstantgame", "/facebookinstantgame"),
        ("POST /v2/account/authenticate/gamecenter", "/gamecenter"),
        ("POST /v2/account/authenticate/google", "/google"),
        ("POST /v2/account/authenticate/steam", "/steam"),
    ] {
        assert_eq!(
            refusal(
                env.client.post(url(path)).json(&json!({ "token": "vendor-token" })).send().await.unwrap(),
                StatusCode::UNAUTHORIZED,
                &format!("{label} without the server key"),
            )
            .await,
            "Server key required",
            "{label}"
        );
    }
    assert_eq!(
        refusal(
            env.client
                .post(url("/apple"))
                .header(reqwest::header::AUTHORIZATION, "Basic bm90LW91ci1rZXk6")
                .json(&json!({ "token": "vendor-token" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "an authenticate with the wrong key",
        )
        .await,
        "Server key invalid"
    );

    // Apple and Steam are gated by configuration before the credential is read.
    assert_eq!(
        refusal(
            env.client
                .post(url("/apple"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "token": "e2e-gap-apple-token" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/authenticate/apple",
        )
        .await,
        "Apple authentication is not configured."
    );
    assert_eq!(
        refusal(
            env.client
                .post(url("/steam"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "token": "e2e-gap-steam-token" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/authenticate/steam",
        )
        .await,
        "Steam authentication is not configured."
    );

    for (label, path, provider) in [
        ("POST /v2/account/authenticate/facebook", "/facebook", "Facebook"),
        ("POST /v2/account/authenticate/google", "/google", "Google"),
    ] {
        assert_eq!(
            refusal(
                env.client
                    .post(url(path))
                    .header(reqwest::header::AUTHORIZATION, &key)
                    .json(&json!({}))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                &format!("{label} with an empty token"),
            )
            .await,
            format!("{provider} access token is required."),
            "{label}"
        );
        assert_eq!(
            refusal(
                env.client
                    .post(url(path))
                    .header(reqwest::header::AUTHORIZATION, &key)
                    .json(&json!({ "token": "e2e-gap-vendor-token" }))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::UNAUTHORIZED,
                label,
            )
            .await,
            format!("Could not authenticate {provider} profile."),
            "{label}"
        );
    }

    assert_eq!(
        refusal(
            env.client
                .post(url("/facebookinstantgame"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/authenticate/facebookinstantgame with no signature",
        )
        .await,
        "Signed Player Info for a Facebook Instant Game is required."
    );
    // An unverifiable signature is DataLoss: BaaS treats it as its own data being
    // wrong, and DataLoss lands on 500.
    assert_eq!(
        refusal(
            env.client
                .post(url("/facebookinstantgame"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "signedPlayerInfo": "e2e-gap-signed" }))
                .send()
                .await
                .unwrap(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "POST /v2/account/authenticate/facebookinstantgame",
        )
        .await,
        "Error extracting the Facebook Instant Game player ID or validating the signature"
    );

    assert_eq!(
        refusal(
            env.client
                .post(url("/gamecenter"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/authenticate/gamecenter with no body",
        )
        .await,
        "GameCenter access credentials are required."
    );
    assert_eq!(
        refusal(
            env.client
                .post(url("/gamecenter"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "the bundle is the first field checked",
        )
        .await,
        "GameCenter bundle ID is required."
    );
    assert_eq!(
        refusal(
            env.client
                .post(url("/gamecenter"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({
                    "bundleId": "com.example.gap",
                    "playerId": "G:gap12345",
                    "timestampSeconds": 1_700_000_000i64,
                    "salt": "gap-salt",
                    "signature": "gap-signature",
                    "publicKeyUrl": "https://example.test/gap-key",
                }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "POST /v2/account/authenticate/gamecenter",
        )
        .await,
        "Could not authenticate GameCenter profile."
    );
}

/// The link and unlink verbs whose credential belongs to a vendor: the checks
/// run to the verification step and stop there, and an empty unlink credential
/// clears the column while the last-way-in guard refuses.
async fn link_unlink_surface(env: &Harness) {
    eprintln!("gap: social link and unlink verbs");
    let player = device_player(&env.client, &env.base, "linke").await;
    let post = |path: &str| {
        env.client
            .post(format!("{}/v2/account/link{path}", env.base))
            .bearer_auth(&player.token)
    };
    let unpost = |path: &str| {
        env.client
            .post(format!("{}/v2/account/unlink{path}", env.base))
            .bearer_auth(&player.token)
    };

    // A link is bearer-authenticated, and the body decodes first.
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/account/link/google", env.base))
                .json(&json!({ "token": "vendor-token" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "a link with no session",
        )
        .await,
        "Auth token required"
    );

    assert_eq!(
        refusal(
            post("/google").json(&json!({})).send().await.unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/link/google with no token",
        )
        .await,
        "Google access token is required."
    );
    assert_eq!(
        refusal(
            post("/google")
                .json(&json!({ "token": "e2e-gap-google-assertion" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "POST /v2/account/link/google",
        )
        .await,
        "Could not authenticate Google profile."
    );
    assert_eq!(
        refusal(
            post("/apple")
                .json(&json!({ "token": "e2e-gap-apple-assertion" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/link/apple",
        )
        .await,
        "Apple authentication is not configured."
    );
    assert_eq!(
        refusal(
            post("/gamecenter").json(&json!({})).send().await.unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/link/gamecenter missing its bundle",
        )
        .await,
        "GameCenter bundle ID is required."
    );
    assert_eq!(
        refusal(
            post("/gamecenter")
                .json(&json!({
                    "bundleId": "com.example.gap",
                    "playerId": "G:gap12345",
                    "timestampSeconds": 1_700_000_000i64,
                    "salt": "gap-salt",
                    "signature": "gap-signature",
                    "publicKeyUrl": "https://example.test/gap-key",
                }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "POST /v2/account/link/gamecenter",
        )
        .await,
        "Could not authenticate GameCenter profile."
    );
    assert_eq!(
        refusal(
            post("/facebookinstantgame").json(&json!({})).send().await.unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/link/facebookinstantgame with no signature",
        )
        .await,
        "Signed Player Info for a Facebook Instant Game is required."
    );
    assert_eq!(
        refusal(
            post("/facebookinstantgame")
                .json(&json!({ "signedPlayerInfo": "e2e-gap-signed" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "POST /v2/account/link/facebookinstantgame",
        )
        .await,
        "Could not authenticate Facebook Instant Game profile."
    );

    // The unlinks: a non-empty credential ends at vendor verification (and the
    // vendor's identity for the instant game link differs from its auth twin);
    // an empty one is the guarded clear.
    for (label, path, provider) in [
        ("unlink apple", "/apple", "Apple"),
        ("unlink facebook", "/facebook", "Facebook"),
        ("unlink google", "/google", "Google"),
    ] {
        assert_eq!(
            refusal(
                unpost(path)
                    .json(&json!({ "token": "e2e-gap-held-credential" }))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::UNAUTHORIZED,
                &format!("{label} with a credential to refine"),
            )
            .await,
            format!("Could not authenticate {provider} profile."),
            "{label}"
        );
    }
    assert_eq!(
        refusal(
            unpost("/facebookinstantgame")
                .json(&json!({ "signedPlayerInfo": "e2e-gap-signed" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "unlink facebookinstantgame with a signature to refine",
        )
        .await,
        "Could not authenticate Facebook Instant Game profile."
    );

    // Empty credentials: the device edge is another way in, so the guard passes
    // and each column is cleared. Seed the facebook id first so the clear is
    // provable rather than vacuous.
    sqlx::query("UPDATE users SET facebook_id = $2 WHERE id = $1")
        .bind(Uuid::parse_str(&player.uid).expect("player uuid"))
        .bind(format!("e2e-gap-fb-{}", player.uid))
        .execute(&env.pool)
        .await
        .expect("seed a facebook id");
    expect(
        unpost("/facebook").json(&json!({})).send().await.unwrap(),
        StatusCode::OK,
        "POST /v2/account/unlink/facebook",
    )
    .await;
    let cleared: Option<String> = sqlx::query_scalar("SELECT facebook_id FROM users WHERE id = $1")
        .bind(Uuid::parse_str(&player.uid).expect("player uuid"))
        .fetch_one(&env.pool)
        .await
        .expect("read the facebook id back");
    assert_eq!(cleared, None, "the empty credential cleared the column");
    expect(
        unpost("/apple").json(&json!({})).send().await.unwrap(),
        StatusCode::OK,
        "POST /v2/account/unlink/apple",
    )
    .await;
    expect(
        unpost("/google").json(&json!({})).send().await.unwrap(),
        StatusCode::OK,
        "POST /v2/account/unlink/google",
    )
    .await;
    expect(
        unpost("/facebookinstantgame")
            .json(&json!({}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/account/unlink/facebookinstantgame",
    )
    .await;

    // The last-way-in guard: strip the device edge off a fresh account and the
    // same call is what the account is saved from.
    let lone = device_player(&env.client, &env.base, "alone").await;
    sqlx::query("DELETE FROM user_device WHERE user_id = $1")
        .bind(Uuid::parse_str(&lone.uid).expect("lone uuid"))
        .execute(&env.pool)
        .await
        .expect("strip the device edge");
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/account/unlink/facebook", env.base))
                .bearer_auth(&lone.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "unlinking the only way in",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn player_groups_boards_channels_iap_and_social_end_to_end() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;

    group_surface(&env).await;
    leaderboard_surface(&env).await;
    tournament_surface(&env, &console).await;
    storage_index_surface(&env).await;
    channel_surface(&env).await;
    iap_surface(&env).await;
    provider_auth_surface(&env).await;
    link_unlink_surface(&env).await;

    env.stop().await;
    eprintln!("gap: player group, leaderboard, tournament, index, channel, iap and social surfaces passed");
}

/// A cursor from the last page must be sent verbatim; keep the DateTime import
/// honest (the harness stamps RFC3339 elsewhere).
#[allow(dead_code)]
fn _stamp_marker(t: DateTime<Utc>) -> String {
    t.to_rfc3339()
}
