use std::collections::HashMap;

use jsonwebtoken::{EncodingKey, Header, encode};
use uuid::Uuid;

use super::claims::Claims;
use super::token::{Keys, Previous, mint_pair_with};
use super::verify::parse;

fn set(key_id: &str, signing: &str, refresh: &str, previous: Option<(&str, &str, &str)>) -> Keys {
    Keys {
        signing: signing.as_bytes().to_vec(),
        refresh_signing: refresh.as_bytes().to_vec(),
        token_expiry_secs: 60,
        refresh_expiry_secs: 3_600,
        key_id: key_id.to_owned(),
        previous: previous.map(|(id, s, r)| Previous {
            key_id: id.to_owned(),
            signing: s.as_bytes().to_vec(),
            refresh_signing: r.as_bytes().to_vec(),
        }),
    }
}

fn uid() -> Uuid {
    Uuid::new_v4()
}

fn minted_with_k1() -> (Uuid, String, String) {
    let uid = uid();
    let (token, refresh) = mint_pair_with(
        &set("k1", "secret-one", "refresh-one", None),
        uid,
        "player",
        HashMap::new(),
    );
    (uid, token, refresh)
}

#[test]
fn a_minted_token_names_the_key_it_expects() {
    let (token, _) = mint_pair_with(
        &set("k2", "secret-two", "refresh-two", None),
        uid(),
        "player",
        HashMap::new(),
    );
    assert_eq!(
        jsonwebtoken::decode_header(&token).expect("header").kid,
        Some("k2".to_owned()),
        "rotation only works if a token says which key signed it"
    );
}

#[test]
fn a_token_minted_under_the_retired_key_verifies_while_the_window_is_open() {
    let (uid, token, refresh) = minted_with_k1();
    let rotated = set(
        "k2",
        "secret-two",
        "refresh-two",
        Some(("k1", "secret-one", "refresh-one")),
    );
    let claims = parse(&token, &rotated, false).expect("the old key is still trusted");
    assert_eq!(claims.uid, uid.to_string());
    assert!(
        parse(&refresh, &rotated, true).is_some(),
        "a refresh token minted under the retired key must not force a re-login"
    );
}

#[test]
fn the_retired_key_stops_working_once_the_window_closes() {
    let (_, token, refresh) = minted_with_k1();
    let retired = set("k2", "secret-two", "refresh-two", None);
    assert!(
        parse(&token, &retired, false).is_none(),
        "one TTL window is the whole promise; after it the old key is gone"
    );
    assert!(parse(&refresh, &retired, true).is_none());
}

#[test]
fn a_key_id_this_process_has_no_secret_for_is_refused_not_guessed() {
    let (_, token, _) = minted_with_k1();
    let rotated = set(
        "k2",
        "secret-two",
        "refresh-two",
        Some(("k1", "secret-one", "refresh-one")),
    );
    assert!(parse(&token, &rotated, false).is_some());
    let unrelated = set("k3", "secret-three", "refresh-three", None);
    assert!(parse(&token, &unrelated, false).is_none());
}

#[test]
fn a_token_minted_before_keys_were_labelled_uses_the_current_key() {
    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        tid: "t".into(),
        uid: uid().to_string(),
        un: "player".into(),
        vars: HashMap::new(),
        exp: now + 60,
        iat: now,
    };
    let legacy = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(b"secret-two"),
    )
    .expect("encodable");
    let current = set("k2", "secret-two", "refresh-two", None);
    assert!(
        parse(&legacy, &current, false).is_some(),
        "upgrading must not log out sessions minted without a kid"
    );
}

#[test]
fn an_unconfigured_previous_secret_verifies_nothing() {
    let (_, token, _) = minted_with_k1();
    let half_configured = set(
        "k2",
        "secret-two",
        "refresh-two",
        Some(("k1", "", "refresh-one")),
    );
    assert!(
        parse(&token, &half_configured, false).is_none(),
        "an empty previous signing key must not become a usable secret"
    );
}
