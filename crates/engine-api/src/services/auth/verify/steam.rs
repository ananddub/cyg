//! Steam Web API `AuthenticateUserTicket`: the ticket must be valid for the
//! configured app id.

use engine_config::SocialConfig;
use serde_json::Value;

use super::Profile;
use crate::error::V2Error;
use crate::utils::vendor;

pub(super) async fn verify(config: &SocialConfig, token: &str) -> Result<Profile, V2Error> {
    let base = config.steam_api_url.trim_end_matches('/');
    let mut url = reqwest::Url::parse(&format!("{base}/ISteamUserAuth/AuthenticateUserTicket/v1/"))
        .map_err(|err| {
            V2Error::unavailable(format!("[control.social] steam_api_url is invalid: {err}"))
        })?;
    url.query_pairs_mut()
        .append_pair("key", &config.steam_web_api_key)
        .append_pair("appid", &config.steam_app_id.to_string())
        .append_pair("ticket", token);
    let response =
        super::client().get(url).send().await.map_err(|err| {
            V2Error::unavailable(format!("Steam ticket verification failed: {err}"))
        })?;
    if !response.status().is_success() {
        return Err(vendor::profile("Steam"));
    }
    let value: Value = response.json().await.map_err(|err| {
        V2Error::unavailable(format!(
            "Steam ticket verification returned an unreadable body: {err}"
        ))
    })?;
    let params = value
        .get("response")
        .and_then(|response| response.get("params"))
        .ok_or_else(|| vendor::profile("Steam"))?;
    let result = params
        .get("result")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !result.eq_ignore_ascii_case("OK") {
        return Err(vendor::profile("Steam"));
    }
    let steam_id = params
        .get("steamid")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| vendor::profile("Steam"))?;
    Ok(Profile {
        id: steam_id.to_owned(),
        display_name: None,
        avatar_url: None,
    })
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use axum::routing::get;
    use axum::{Json, Router};
    use serde_json::{Value, json};

    use super::*;

    async fn serve(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind");
        let port = listener.local_addr().expect("address").port();
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("serve");
        });
        format!("http://127.0.0.1:{port}")
    }

    fn config(base: &str) -> SocialConfig {
        SocialConfig {
            steam_app_id: 480,
            steam_web_api_key: "steam-key".into(),
            steam_api_url: base.into(),
            ..SocialConfig::default()
        }
    }

    async fn ok(Query(query): Query<std::collections::HashMap<String, String>>) -> Json<Value> {
        assert_eq!(query.get("key").map(String::as_str), Some("steam-key"));
        assert_eq!(query.get("appid").map(String::as_str), Some("480"));
        assert_eq!(query.get("ticket").map(String::as_str), Some("ticket"));
        Json(json!({
            "response": { "params": { "result": "OK", "steamid": "76561198000000001" } },
        }))
    }

    async fn invalid() -> Json<Value> {
        Json(json!({ "response": { "params": { "result": "InvalidTicket" } } }))
    }

    #[tokio::test]
    async fn a_valid_ticket_resolves_the_steam_id() {
        let base =
            serve(Router::new().route("/ISteamUserAuth/AuthenticateUserTicket/v1/", get(ok))).await;
        let profile = verify(&config(&base), "ticket").await.expect("verified");
        assert_eq!(profile.id, "76561198000000001");
    }

    #[tokio::test]
    async fn an_invalid_ticket_is_refused() {
        let base =
            serve(Router::new().route("/ISteamUserAuth/AuthenticateUserTicket/v1/", get(invalid)))
                .await;
        let error = verify(&config(&base), "ticket")
            .await
            .expect_err("invalid ticket");
        assert_eq!(error.message, "Could not authenticate Steam profile.");
    }
}
