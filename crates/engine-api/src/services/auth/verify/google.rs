//! Google ID-token introspection (`tokeninfo?id_token=…`).

use engine_config::SocialConfig;
use serde_json::Value;

use super::Profile;
use crate::error::V2Error;
use crate::utils::vendor;

pub(super) async fn verify(config: &SocialConfig, token: &str) -> Result<Profile, V2Error> {
    let mut url = reqwest::Url::parse(&config.google_tokeninfo_url).map_err(|err| {
        V2Error::unavailable(format!(
            "[control.social] google_tokeninfo_url is invalid: {err}"
        ))
    })?;
    url.query_pairs_mut().append_pair("id_token", token);
    let response = super::client()
        .get(url)
        .send()
        .await
        .map_err(|err| V2Error::unavailable(format!("Google token verification failed: {err}")))?;
    if !response.status().is_success() {
        return Err(vendor::profile("Google"));
    }
    let value: Value = response.json().await.map_err(|err| {
        V2Error::unavailable(format!(
            "Google token verification returned an unreadable body: {err}"
        ))
    })?;
    let audience = value.get("aud").and_then(Value::as_str).unwrap_or_default();
    if !config.google_client_ids.iter().any(|id| id == audience) {
        return Err(vendor::profile("Google"));
    }
    if expiry(&value) <= chrono::Utc::now().timestamp() {
        return Err(vendor::profile("Google"));
    }
    let id = value
        .get("sub")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| vendor::profile("Google"))?;
    Ok(Profile {
        id: id.to_owned(),
        display_name: text(&value, "name"),
        avatar_url: text(&value, "picture"),
    })
}

fn expiry(value: &Value) -> i64 {
    value
        .get("exp")
        .and_then(|exp| exp.as_i64().or_else(|| exp.as_str()?.parse().ok()))
        .unwrap_or(0)
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
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
            google_client_ids: vec!["client-1".into()],
            google_tokeninfo_url: format!("{base}/tokeninfo"),
            ..SocialConfig::default()
        }
    }

    async fn tokeninfo(Query(query): Query<std::collections::HashMap<String, String>>) -> Json<Value> {
        assert_eq!(query.get("id_token").map(String::as_str), Some("google-token"));
        Json(json!({
            "aud": "client-1",
            "sub": "google-user-1",
            "exp": chrono::Utc::now().timestamp() + 600,
            "name": "Ada",
            "picture": "https://example.test/ada.png",
        }))
    }

    #[tokio::test]
    async fn a_valid_id_token_resolves_the_google_subject() {
        let base = serve(Router::new().route("/tokeninfo", get(tokeninfo))).await;
        let profile = verify(&config(&base), "google-token").await.expect("verified");
        assert_eq!(profile.id, "google-user-1");
        assert_eq!(profile.display_name.as_deref(), Some("Ada"));
        assert_eq!(profile.avatar_url.as_deref(), Some("https://example.test/ada.png"));
    }

    #[tokio::test]
    async fn another_client_audience_is_refused() {
        async fn other() -> Json<Value> {
            Json(json!({
                "aud": "someone-else",
                "sub": "google-user-1",
                "exp": chrono::Utc::now().timestamp() + 600,
            }))
        }
        let base = serve(Router::new().route("/tokeninfo", get(other))).await;
        let error = verify(&config(&base), "google-token")
            .await
            .expect_err("wrong audience");
        assert_eq!(error.message, "Could not authenticate Google profile.");
    }

    #[tokio::test]
    async fn an_expired_id_token_is_refused() {
        async fn expired() -> Json<Value> {
            Json(json!({
                "aud": "client-1",
                "sub": "google-user-1",
                "exp": chrono::Utc::now().timestamp() - 60,
            }))
        }
        let base = serve(Router::new().route("/tokeninfo", get(expired))).await;
        let error = verify(&config(&base), "google-token")
            .await
            .expect_err("expired");
        assert_eq!(error.message, "Could not authenticate Google profile.");
    }
}
