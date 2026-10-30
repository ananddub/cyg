use std::collections::HashMap;

use axum::extract::{Form, Path};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use super::*;

const PRODUCT_PATH: &str =
    "/androidpublisher/v3/applications/{package}/purchases/products/{product}/tokens/{token}";
const SUB_PATH: &str =
    "/androidpublisher/v3/applications/{package}/purchases/subscriptionsv2/tokens/{token}";

/// A throwaway RSA key; only the mock token endpoint ever sees its assertion.
const TEST_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDkvzZqjas8/QCK
hQNVxWoHyw4zeiWPYiFthKjJCbVvgTWqMuY90ZwaOC1cFOVesVHwMwcn396dZF4l
PthJJHmq+QEmeDdKmIUWDrcwgE7E+dJ8UClJYkSrsahqqchfcLBu1tbF0OWj88Nl
4QZrIrGif4SzH9LOj2QJq8r1wsYThBlWzK+t4LUyo24KpZ7cvEDw4C1jA/MAC9C5
KufzacP6LnoZoPIlbAeKqtc9pdoTMY4dOUQPSVnWMDS2DsBGP/GRi8lNMcl7aas3
/ZQSvs3/tzPh5cS0nO9QbZ/OvX7xaeVPtwoR8HMgWDmwHCWMj6lSV1ZjTQJ2lV7J
dSoBgeDHAgMBAAECggEABi7i/itZKxtdpeDlj6536MEE38BX/pG0/Jxe4ZxJErwH
4tNgZOBjG1RUaY5JCu+5GsiOU3YaMt/xKYmS2BtLzKLRdKRAPlx0EH4SVpXJS0rD
dUfHbevhllCFkGiJeYb2xfA5uvwkEPP4Xqsf/MphydYvlJXyIVI/7oD9yfnbV9Cn
s30X0QbpAMYsro6eJCOIO0/DWB34IcSUmuP/vr+UA2o9SrL9f9SH8AMYkkE4fiT5
TyZyW5qmL3o+HL78PJNCZCh1XBqzzmkOcEcJQTE3FMJ8H6PPe/bXdfbNfmQrxF2P
Rl3eU6zZR6hBk+wN4QO11BPpUztOOcO6F7oJnmM7aQKBgQD+QcAvW3a+IweRDcQJ
yY7Nc0jNfJgM7zVjwzr/Mt0Q71Ru9SEOZtzXOlv1baJ4Y5VigdJGKCvJyPRR4Yy6
DyIENzEoTrtuUiYZSYnQWbM/aQD+fjyvhu0V7XrVeUgUc/NQYBW1AZ1bDEUg0YFi
QtaLH02TmvWnH3YrPlFgGT6YLwKBgQDmULBncuVfXutZqq8MHUSp69p6JWH2uPHU
JS1Oc5+XqJ7WMDyE5jkYGhynFzbvnVWVcWKVRR5Qz2TXfNkqn4aztIzX+JnGJ582
HoFqhueIz9sl1h9GMzQWQODOECRlI1BWqaqgDu0mBdXKcmtkaDOvdn1iFaKnIDb+
cVNtW/cC6QKBgHA+jp1Zf35I8Jo0vYqklIpWjplIm8d6AtPb6DupNLa5MUkaplE5
T90kQlyC3erP667lKPMX4Nzf/hv3uqyY6ad6xhTJTS1SzsbzFAgK0Xr43Bt9l0TW
B+xaPpPnSs7csVmWUX7rC5CaRhpK4xeB8jzQwOqcyj3RQdWtWTdXba65AoGAVv0o
85mo2rGuMKrP/MJdOdS0KyhNbPODmz4Dc7smDxEIFRwY75VJATpzkgSpoi0Jsy0l
L/3wKlN64gzstTWVmfBatgPWiMpUTwuMk+CqOLlRK2RJYalbRgBxKENei7if0mu4
30Vj+tV/XzyCJWVaCC63wsErMvZu7wF8ojR3AmkCgYEAw5HwRR52NEXl3VNJny/I
m8X/4bVLfJe9fwErCgtcTS7Y+TusKvpY5NnUWLx4gBrpQJBP9qTbTpz89juLYxyR
bkul1eIejMmcL7sQRdaUosDdZsvvkqNdSyUrPyoow1cK2YWTjWl+JelfR0tjhVME
ATQbviS9erlYt999CoEfVNY=
-----END PRIVATE KEY-----
"#;

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

fn config(base: &str) -> IapConfig {
    IapConfig {
        google_service_account: json!({
            "client_email": "verifier@example.iam.gserviceaccount.com",
            "private_key": TEST_KEY,
            "token_uri": format!("{base}/token"),
        })
        .to_string(),
        google_package_name: "com.example.game".into(),
        google_token_url: format!("{base}/token"),
        google_api_base: format!("{base}/androidpublisher/v3"),
        ..IapConfig::default()
    }
}

async fn token(Form(form): Form<HashMap<String, String>>) -> Json<Value> {
    assert_eq!(
        form.get("grant_type").map(String::as_str),
        Some("urn:ietf:params:oauth:grant-type:jwt-bearer")
    );
    let assertion = form.get("assertion").expect("an assertion");
    assert_eq!(assertion.split('.').count(), 3, "a JWT has three parts");
    Json(json!({ "access_token": "test-access", "expires_in": 3600 }))
}

fn authorized(headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        == Some("Bearer test-access")
}

async fn product(headers: HeaderMap) -> Json<Value> {
    assert!(authorized(&headers));
    Json(json!({
        "purchaseState": 0,
        "orderId": "GPA.1234-5678",
        "purchaseTimeMillis": "1700000000000",
    }))
}

async fn canceled(headers: HeaderMap) -> Json<Value> {
    assert!(authorized(&headers));
    Json(json!({ "purchaseState": 1, "orderId": "GPA.1" }))
}

async fn missing() -> StatusCode {
    StatusCode::NOT_FOUND
}

async fn subscription(
    headers: HeaderMap,
    Path((_package, _token)): Path<(String, String)>,
) -> Json<Value> {
    assert!(authorized(&headers));
    Json(json!({
        "subscriptionState": "SUBSCRIPTION_STATE_ACTIVE",
        "latestOrderId": "GPA.9999",
        "startTime": "2024-01-01T00:00:00Z",
        "lineItems": [{ "productId": "season.gold", "expiryTime": "2024-02-01T00:00:00Z" }],
    }))
}

#[tokio::test]
async fn a_product_purchase_is_verified_through_the_service_account() {
    let app = Router::new()
        .route("/token", post(token))
        .route(PRODUCT_PATH, get(product));
    let base = serve(app).await;
    let purchase = verify(&config(&base), "purchase-token", Some("gem.large"))
        .await
        .expect("verified");
    assert_eq!(purchase.transaction_id, "GPA.1234-5678");
    assert_eq!(purchase.product_id, "gem.large");
    assert_eq!(purchase.environment, 2);
    assert_eq!(
        purchase.purchase_time.expect("purchase time").timestamp(),
        1_700_000_000
    );
}

#[tokio::test]
async fn an_unknown_product_falls_back_to_the_subscription_lookup() {
    let app = Router::new()
        .route("/token", post(token))
        .route(PRODUCT_PATH, get(missing))
        .route(SUB_PATH, get(subscription));
    let base = serve(app).await;
    let purchase = verify(&config(&base), "subscription-token", Some("season.gold"))
        .await
        .expect("verified");
    assert_eq!(purchase.transaction_id, "GPA.9999");
    assert_eq!(purchase.product_id, "season.gold");
    assert!(purchase.expire_time.is_some());
    assert_eq!(
        purchase.expire_time.expect("expiry").timestamp(),
        1_706_745_600
    );
}

#[tokio::test]
async fn a_canceled_purchase_is_refused() {
    let app = Router::new()
        .route("/token", post(token))
        .route(PRODUCT_PATH, get(canceled));
    let base = serve(app).await;
    let error = verify(&config(&base), "purchase-token", Some("gem.large"))
        .await
        .expect_err("Google reports the purchase as canceled");
    assert_eq!(error.kind, crate::error::FAILED_PRECONDITION);
    assert!(error.message.contains("purchase state 1"));
}

#[tokio::test]
async fn a_service_account_that_is_not_json_is_refused() {
    let config = IapConfig {
        google_service_account: "not json".into(),
        google_package_name: "com.example.game".into(),
        ..IapConfig::default()
    };
    let error = verify(&config, "purchase-token", None)
        .await
        .expect_err("no service account");
    assert!(error.message.contains("service-account JSON"));
}
