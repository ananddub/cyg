use super::*;

const KEY: &str = "gap-ab-test";
const SALT: &str = "gap-salt";

async fn get(env: &Harness, token: &str, path: &str) -> reqwest::Response {
    env.client
        .get(format!("{}{path}", env.base))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
}

async fn post(env: &Harness, token: &str, path: &str, body: Value) -> reqwest::Response {
    env.client
        .post(format!("{}{path}", env.base))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn put(env: &Harness, token: &str, path: &str, body: Value) -> reqwest::Response {
    env.client
        .put(format!("{}{path}", env.base))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn del(env: &Harness, token: &str, path: &str) -> reqwest::Response {
    env.client
        .delete(format!("{}{path}", env.base))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
}

/// A flag at full rollout split across two equal arms, which is the shape every
/// surface here starts from.
async fn open(env: &Harness, admin: &str) {
    let row = json_body(
        post(
            env,
            admin,
            "/v2/console/flags",
            json!({
                "flag_key": KEY,
                "description": "Two builds of the loot screen.",
                "enabled": true,
                "rollout_pct": 100,
                "salt": SALT,
            }),
        )
        .await,
        StatusCode::OK,
        "a defined experiment",
    )
    .await;
    assert_eq!(
        (
            row["flag_key"].as_str(),
            row["rollout_pct"].as_u64(),
            row["salt"].as_str(),
        ),
        (Some(KEY), Some(100), Some(SALT)),
        "{row}"
    );
    for (name, weight) in [("control", 50), ("treatment", 50)] {
        let arm = json_body(
            post(
                env,
                admin,
                &format!("/v2/console/flags/{KEY}/variants"),
                json!({ "name": name, "weight": weight }),
            )
            .await,
            StatusCode::OK,
            &format!("the {name} arm"),
        )
        .await;
        assert_eq!(arm["is_control"], json!(name == "control"), "{arm}");
    }
}

/// What one account was told about the experiment.
async fn told(env: &Harness, player: &str) -> (String, bool) {
    let body = json_body(
        get(env, player, "/v2/flags").await,
        StatusCode::OK,
        "the player's flags",
    )
    .await;
    let told = body["flags"]
        .as_array()
        .expect("a flag list")
        .iter()
        .find(|flag| flag["key"] == json!(KEY))
        .unwrap_or_else(|| panic!("nothing was told about {KEY}: {body}"));
    (
        told["variant"].as_str().expect("an arm name").to_owned(),
        told["on"].as_bool().expect("a decision"),
    )
}

async fn players(env: &Harness, count: usize) -> Vec<Player> {
    let mut out = Vec::new();
    for index in 0..count {
        out.push(device_player(&env.client, &env.base, &format!("ab{index}")).await);
    }
    out
}

pub async fn assignment_surface(env: &Harness) {
    let admin = console_login(&env.client, &env.base).await;
    open(env, &admin).await;

    let arms = json_body(
        get(env, &admin, &format!("/v2/console/flags/{KEY}/variants")).await,
        StatusCode::OK,
        "the arm list",
    )
    .await;
    let arms = arms.as_array().expect("an arm list");
    assert_eq!(arms.len(), 2, "{arms:?}");
    assert_eq!(arms[0]["name"], json!("control"));
    assert_eq!(arms[0]["is_control"], json!(true));

    assert_eq!(
        refusal(
            post(env, &admin, "/v2/console/flags", json!({ "flag_key": KEY })).await,
            StatusCode::CONFLICT,
            "a second flag under the same key",
        )
        .await,
        "A flag with this key already exists."
    );
    assert_eq!(
        refusal(
            post(
                env,
                &admin,
                "/v2/console/flags",
                json!({ "flag_key": "Not A Key" })
            )
            .await,
            StatusCode::BAD_REQUEST,
            "a key that is not one",
        )
        .await,
        "A flag key is 1-64 characters of lowercase letters, digits, dash or underscore."
    );

    let crowd = players(env, 12).await;
    let mut seen = vec![];
    for player in &crowd {
        let (arm, on) = told(env, &player.token).await;
        assert_eq!(on, arm == "treatment", "{}: told {arm}", player.uid);
        for _ in 0..2 {
            assert_eq!(
                told(env, &player.token).await.0,
                arm,
                "{}: moved arms between two asks",
                player.uid
            );
        }
        seen.push(arm);
    }
    // Twelve accounts at fifty-fifty is enough to show the split is a split: the
    // chance of all twelve landing in one arm is one in four thousand.
    assert!(
        seen.iter().any(|arm| arm == "control") && seen.iter().any(|arm| arm == "treatment"),
        "{seen:?}"
    );

    assert_eq!(
        env.client
            .get(format!("{}/v2/flags", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED,
        "an anonymous flag list"
    );
}

pub async fn rollout_surface(env: &Harness) {
    let admin = console_login(&env.client, &env.base).await;
    open(env, &admin).await;
    let crowd = players(env, 8).await;

    let stopped = json_body(
        post(
            env,
            &admin,
            &format!("/v2/console/flags/{KEY}/stop"),
            json!({}),
        )
        .await,
        StatusCode::OK,
        "stopping the experiment",
    )
    .await;
    assert_eq!(stopped["rollout_pct"], json!(0), "{stopped}");
    for player in &crowd {
        assert_eq!(
            told(env, &player.token).await,
            ("control".to_owned(), false),
            "{}: still in a stopped experiment",
            player.uid
        );
    }

    let pinned = &crowd[0];
    let pin = json_body(
        put(
            env,
            &admin,
            &format!("/v2/console/flags/{KEY}/overrides"),
            json!({ "user_id": pinned.uid, "variant": "treatment" }),
        )
        .await,
        StatusCode::OK,
        "pinning an account",
    )
    .await;
    assert_eq!(pin["variant"], json!("treatment"), "{pin}");
    assert_eq!(
        json_body(
            get(env, &admin, &format!("/v2/console/flags/{KEY}/overrides")).await,
            StatusCode::OK,
            "the pin list",
        )
        .await
        .as_array()
        .expect("a pin list")
        .len(),
        1
    );
    assert_eq!(
        told(env, &pinned.token).await,
        ("treatment".to_owned(), true),
        "a pinned account is in the test while the rollout is at zero"
    );

    json_body(
        post(
            env,
            &admin,
            &format!("/v2/console/flags/{KEY}/rollout"),
            json!({ "percent": 30 }),
        )
        .await,
        StatusCode::OK,
        "rolling out to thirty percent",
    )
    .await;
    assert_eq!(
        told(env, &pinned.token).await,
        ("treatment".to_owned(), true),
        "a pin survives a rollout change"
    );

    // The kill switch outranks everything else defined against the flag, pin
    // included: it is the one control that guarantees nobody sees the new build.
    let off = json_body(
        put(
            env,
            &admin,
            &format!("/v2/console/flags/{KEY}"),
            json!({ "enabled": false }),
        )
        .await,
        StatusCode::OK,
        "switching the flag off",
    )
    .await;
    assert_eq!(off["enabled"], json!(false), "{off}");
    assert_eq!(off["rollout_pct"], json!(30), "{off}");
    assert_eq!(
        off["description"].as_str(),
        Some("Two builds of the loot screen."),
        "a PUT that says nothing about the description keeps it"
    );
    for player in &crowd {
        assert_eq!(
            told(env, &player.token).await,
            ("control".to_owned(), false),
            "{}: saw the experiment through a switched-off flag",
            player.uid
        );
    }

    json_body(
        put(
            env,
            &admin,
            &format!("/v2/console/flags/{KEY}"),
            json!({ "enabled": true }),
        )
        .await,
        StatusCode::OK,
        "switching the flag back on",
    )
    .await;
    assert_eq!(
        told(env, &pinned.token).await,
        ("treatment".to_owned(), true),
        "the pin is still there"
    );

    assert_eq!(
        refusal(
            put(
                env,
                &admin,
                &format!("/v2/console/flags/{KEY}/overrides"),
                json!({ "user_id": crowd[1].uid, "variant": "gone" }),
            )
            .await,
            StatusCode::BAD_REQUEST,
            "a pin to an arm nobody defined",
        )
        .await,
        "This flag has no arm with that name."
    );

    json_body(
        del(
            env,
            &admin,
            &format!("/v2/console/flags/{KEY}/overrides/{}", pinned.uid),
        )
        .await,
        StatusCode::OK,
        "releasing the pin",
    )
    .await;
    assert_eq!(
        refusal(
            del(
                env,
                &admin,
                &format!("/v2/console/flags/{KEY}/overrides/{}", pinned.uid),
            )
            .await,
            StatusCode::NOT_FOUND,
            "releasing a pin that is gone",
        )
        .await,
        "Override not found."
    );
    let (arm, on) = told(env, &pinned.token).await;
    assert_eq!(
        on,
        arm == "treatment",
        "a released pin follows the hash: {arm}"
    );

    // A scheduled experiment is a door on the same flag: outside its window the
    // control for everybody, and the accounts it drew come back inside it.
    let past = (chrono::Utc::now() + chrono::Duration::seconds(-60)).to_rfc3339();
    let future = (chrono::Utc::now() + chrono::Duration::seconds(3600)).to_rfc3339();
    let closed = json_body(
        put(
            env,
            &admin,
            &format!("/v2/console/flags/{KEY}"),
            json!({ "active_to": past }),
        )
        .await,
        StatusCode::OK,
        "scheduling an experiment that has already closed",
    )
    .await;
    assert!(closed["active_to"].is_string(), "{closed}");
    assert_eq!(
        closed["rollout_pct"],
        json!(30),
        "a window is not a rollout"
    );
    for player in &crowd {
        assert_eq!(
            told(env, &player.token).await,
            ("control".to_owned(), false),
            "{}: inside a window that closed",
            player.uid
        );
    }

    assert_eq!(
        refusal(
            put(
                env,
                &admin,
                &format!("/v2/console/flags/{KEY}"),
                json!({ "active_from": future, "active_to": past }),
            )
            .await,
            StatusCode::BAD_REQUEST,
            "a window that closes before it opens",
        )
        .await,
        "A flag's window has to open before it closes."
    );

    json_body(
        put(
            env,
            &admin,
            &format!("/v2/console/flags/{KEY}"),
            json!({ "active_to": future }),
        )
        .await,
        StatusCode::OK,
        "opening the window again",
    )
    .await;
    for player in &crowd {
        let (arm, on) = told(env, &player.token).await;
        assert_eq!(
            on,
            arm == "treatment",
            "{}: back in the test through an open window: {arm}",
            player.uid
        );
    }
}

pub async fn permission_surface(env: &Harness) {
    let admin = console_login(&env.client, &env.base).await;
    open(env, &admin).await;

    assert_eq!(
        env.client
            .get(format!("{}/v2/console/flags", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED,
        "the console flag list with no token"
    );

    let developer = operator(ConsoleRole::Developer);
    assert_eq!(
        refusal(
            post(
                env,
                &developer,
                &format!("/v2/console/flags/{KEY}/rollout"),
                json!({ "percent": 80 }),
            )
            .await,
            StatusCode::FORBIDDEN,
            "a writer rolling past half the population",
        )
        .await,
        "This console action requires the admin role."
    );
    assert_eq!(
        refusal(
            post(
                env,
                &developer,
                "/v2/console/flags",
                json!({ "flag_key": "gap-other", "rollout_pct": 75 }),
            )
            .await,
            StatusCode::FORBIDDEN,
            "a writer creating a flag past half the population",
        )
        .await,
        "This console action requires the admin role."
    );
    json_body(
        post(
            env,
            &developer,
            &format!("/v2/console/flags/{KEY}/rollout"),
            json!({ "percent": 50 }),
        )
        .await,
        StatusCode::OK,
        "a writer rolling to exactly half",
    )
    .await;
    json_body(
        post(
            env,
            &admin,
            &format!("/v2/console/flags/{KEY}/rollout"),
            json!({ "percent": 80 }),
        )
        .await,
        StatusCode::OK,
        "the admin rolling past half",
    )
    .await;

    assert_eq!(
        refusal(
            post(
                env,
                &admin,
                &format!("/v2/console/flags/{KEY}/rollout"),
                json!({ "percent": 101 }),
            )
            .await,
            StatusCode::BAD_REQUEST,
            "a rollout that is not a percentage",
        )
        .await,
        "A rollout is a percentage between 0 and 100."
    );
    assert_eq!(
        refusal(
            post(
                env,
                &admin,
                &format!("/v2/console/flags/{KEY}/variants"),
                json!({ "name": "heavy", "weight": 5_000 }),
            )
            .await,
            StatusCode::BAD_REQUEST,
            "an arm nobody could weigh",
        )
        .await,
        "An arm's weight is between 0 and 1000."
    );

    let reader = operator(ConsoleRole::Readonly);
    assert_eq!(
        refusal(
            post(
                env,
                &reader,
                "/v2/console/flags",
                json!({ "flag_key": "gap-readonly" }),
            )
            .await,
            StatusCode::FORBIDDEN,
            "a read-only console user defining an experiment",
        )
        .await,
        "This console user has read-only access."
    );
    json_body(
        get(env, &reader, &format!("/v2/console/flags/{KEY}/variants")).await,
        StatusCode::OK,
        "a read-only console user reading the arms",
    )
    .await;
}

pub async fn delete_surface(env: &Harness) {
    let admin = console_login(&env.client, &env.base).await;
    open(env, &admin).await;
    let crowd = players(env, 2).await;
    json_body(
        put(
            env,
            &admin,
            &format!("/v2/console/flags/{KEY}/overrides"),
            json!({ "user_id": crowd[0].uid, "variant": "treatment" }),
        )
        .await,
        StatusCode::OK,
        "pinning an account",
    )
    .await;

    expect(
        del(env, &admin, &format!("/v2/console/flags/{KEY}")).await,
        StatusCode::OK,
        "deleting the experiment",
    )
    .await;
    assert_eq!(count(env.db(), "feature_flag", None, &[]).await, 0);
    assert_eq!(count(env.db(), "flag_variant", None, &[]).await, 0);
    assert_eq!(count(env.db(), "flag_override", None, &[]).await, 0);
    assert_eq!(
        json_body(
            get(env, &crowd[0].token, "/v2/flags").await,
            StatusCode::OK,
            "the player's flags after the delete",
        )
        .await["flags"]
            .as_array()
            .expect("a flag list")
            .len(),
        0
    );

    assert_eq!(
        refusal(
            del(env, &admin, &format!("/v2/console/flags/{KEY}")).await,
            StatusCode::NOT_FOUND,
            "deleting an experiment twice",
        )
        .await,
        "Flag not found."
    );
    assert_eq!(
        refusal(
            post(
                env,
                &admin,
                &format!("/v2/console/flags/{KEY}/rollout"),
                json!({ "percent": 10 }),
            )
            .await,
            StatusCode::NOT_FOUND,
            "rolling out an experiment that is gone",
        )
        .await,
        "Flag not found."
    );
    assert_eq!(
        refusal(
            put(
                env,
                &admin,
                &format!("/v2/console/flags/{KEY}/variants/{}", "control"),
                json!({ "weight": 1 }),
            )
            .await,
            StatusCode::NOT_FOUND,
            "retuning an experiment that is gone",
        )
        .await,
        "Flag not found."
    );
}
