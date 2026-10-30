//! OAuth2 service-account grant: sign a JWT assertion, exchange it for a token.

use engine_config::IapConfig;
use serde::Deserialize;

use crate::error::V2Error;

const SCOPE: &str = "https://www.googleapis.com/auth/androidpublisher";

#[derive(Deserialize)]
struct ServiceAccount {
    client_email: String,
    private_key: String,
    #[serde(default)]
    token_uri: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

pub(super) async fn access_token(config: &IapConfig) -> Result<String, V2Error> {
    let account: ServiceAccount =
        serde_json::from_str(&config.google_service_account).map_err(|err| {
            V2Error::unavailable(format!(
                "[control.iap] google_service_account is not a service-account JSON document: {err}"
            ))
        })?;
    let token_uri = account
        .token_uri
        .filter(|uri| !uri.is_empty())
        .unwrap_or_else(|| config.google_token_url.clone());
    let now = chrono::Utc::now().timestamp();
    let claims = serde_json::json!({
        "iss": account.client_email,
        "scope": SCOPE,
        "aud": token_uri,
        "iat": now,
        "exp": now + 3_600,
    });
    let key =
        jsonwebtoken::EncodingKey::from_rsa_pem(account.private_key.as_bytes()).map_err(|err| {
            V2Error::unavailable(format!(
                "[control.iap] google_service_account private_key is not a PEM RSA key: {err}"
            ))
        })?;
    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    let assertion = jsonwebtoken::encode(&header, &claims, &key)
        .map_err(|err| V2Error::internal(format!("Failed to sign the Google assertion: {err}")))?;
    let response = super::super::client()
        .post(&token_uri)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", assertion.as_str()),
        ])
        .send()
        .await
        .map_err(|err| V2Error::unavailable(format!("Google token request failed: {err}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(V2Error::unavailable(format!(
            "Google token endpoint answered {status}."
        )));
    }
    let token: TokenResponse = response.json().await.map_err(|err| {
        V2Error::unavailable(format!("Google token response was unreadable: {err}"))
    })?;
    Ok(token.access_token)
}
