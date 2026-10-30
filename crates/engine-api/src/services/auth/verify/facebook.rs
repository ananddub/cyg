//! Facebook `debug_token`: the access token must be valid for our app.

use engine_config::SocialConfig;
use serde_json::Value;

use super::Profile;
use crate::error::V2Error;
use crate::utils::vendor;

pub(super) async fn verify(config: &SocialConfig, token: &str) -> Result<Profile, V2Error> {
    let base = config.facebook_graph_url.trim_end_matches('/');
    let mut url = reqwest::Url::parse(&format!("{base}/debug_token")).map_err(|err| {
        V2Error::unavailable(format!(
            "[control.social] facebook_graph_url is invalid: {err}"
        ))
    })?;
    url.query_pairs_mut()
        .append_pair("input_token", token)
        .append_pair(
            "access_token",
            &format!("{}|{}", config.facebook_app_id, config.facebook_app_secret),
        );
    let response = super::client()
        .get(url)
        .send()
        .await
        .map_err(|err| V2Error::unavailable(format!("Facebook token verification failed: {err}")))?;
    if !response.status().is_success() {
        return Err(vendor::profile("Facebook"));
    }
    let value: Value = response.json().await.map_err(|err| {
        V2Error::unavailable(format!(
            "Facebook token verification returned an unreadable body: {err}"
        ))
    })?;
    let data = value
        .get("data")
        .ok_or_else(|| vendor::profile("Facebook"))?;
    let valid = data
        .get("is_valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let app_id = data
        .get("app_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !valid || app_id != config.facebook_app_id {
        return Err(vendor::profile("Facebook"));
    }
    let user_id = data
        .get("user_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| vendor::profile("Facebook"))?;
    Ok(Profile {
        id: user_id.to_owned(),
        display_name: None,
        avatar_url: None,
    })
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use axum::routing::get;
    use axum::{Json, Router};
    use serde_json::{json, Value};

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
            facebook_app_id: "app-1".into(),
            facebook_app_secret: "secret".into(),
            facebook_graph_url: base.into(),
            ..SocialConfig::default()
        }
    }

    async fn valid(Query(query): Query<std::collections::HashMap<String, String>>) -> Json<Value> {
        assert_eq!(
            query.get("access_token").map(String::as_str),
            Some("app-1|secret")
        );
        assert_eq!(query.get("input_token").map(String::as_str), Some("user-token"));
        Json(json!({
            "data": { "app_id": "app-1", "is_valid": true, "user_id": "fb-user-1" },
        }))
    }

    async fn wrong_app() -> Json<Value> {
        Json(json!({
            "data": { "app_id": "app-2", "is_valid": true, "user_id": "fb-user-1" },
        }))
    }

    #[tokio::test]
    async fn a_valid_token_resolves_the_facebook_user() {
        let base = serve(Router::new().route("/debug_token", get(valid))).await;
        let profile = verify(&config(&base), "user-token").await.expect("verified");
        assert_eq!(profile.id, "fb-user-1");
    }

    #[tokio::test]
    async fn a_token_issued_to_another_app_is_refused() {
        let base = serve(Router::new().route("/debug_token", get(wrong_app))).await;
        let error = verify(&config(&base), "user-token")
            .await
            .expect_err("another app");
        assert_eq!(error.message, "Could not authenticate Facebook profile.");
    }
}
