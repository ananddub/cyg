//! Apple `verifyReceipt`: production first, sandbox when Apple answers 21007.

use engine_config::IapConfig;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{VerifiedPurchase, parse_ms};
use crate::error::V2Error;

#[derive(Deserialize)]
struct AppleResponse {
    status: i64,
    #[serde(default)]
    receipt: Option<AppleReceipt>,
    #[serde(default)]
    latest_receipt_info: Vec<ApplePurchase>,
}

#[derive(Deserialize)]
struct AppleReceipt {
    #[serde(default)]
    in_app: Vec<ApplePurchase>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApplePurchase {
    #[serde(default)]
    transaction_id: String,
    #[serde(default)]
    product_id: String,
    #[serde(default)]
    purchase_date_ms: String,
    #[serde(default)]
    expires_date_ms: Option<String>,
}

pub(super) async fn verify(
    config: &IapConfig,
    receipt: &str,
    product_id: Option<&str>,
    transaction_id: Option<&str>,
) -> Result<VerifiedPurchase, V2Error> {
    let body = json!({
        "receipt-data": receipt,
        "password": config.apple_shared_secret,
        "exclude-old-transactions": true,
    });
    let mut value = post(&config.apple_verify_url, &body).await?;
    let mut parsed = parse(&value)?;
    let mut environment = 2;
    if parsed.status == 21007 {
        value = post(&config.apple_sandbox_url, &body).await?;
        parsed = parse(&value)?;
        environment = 1;
    }
    if parsed.status != 0 {
        return Err(V2Error::failed_precondition(format!(
            "Apple rejected the receipt (status {}).",
            parsed.status
        )));
    }
    let mut purchases = parsed.latest_receipt_info;
    if let Some(receipt) = parsed.receipt {
        purchases.extend(receipt.in_app);
    }
    let purchase = select(purchases, product_id, transaction_id)?;
    Ok(VerifiedPurchase {
        transaction_id: purchase.transaction_id.clone(),
        product_id: purchase.product_id.clone(),
        environment,
        purchase_time: parse_ms(&purchase.purchase_date_ms),
        expire_time: purchase.expires_date_ms.as_deref().and_then(parse_ms),
        raw: value,
    })
}

async fn post(url: &str, body: &Value) -> Result<Value, V2Error> {
    let response = super::client()
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(|err| V2Error::unavailable(format!("Apple verification request failed: {err}")))?;
    let status = response.status();
    let value: Value = response.json().await.map_err(|err| {
        V2Error::unavailable(format!(
            "Apple verification returned an unreadable body: {err}"
        ))
    })?;
    if !status.is_success() && value.get("status").is_none() {
        return Err(V2Error::unavailable(format!(
            "Apple verification answered {status}."
        )));
    }
    Ok(value)
}

fn parse(value: &Value) -> Result<AppleResponse, V2Error> {
    serde_json::from_value(value.clone()).map_err(|err| {
        V2Error::unavailable(format!(
            "Apple verification returned an unexpected body: {err}"
        ))
    })
}

fn select(
    purchases: Vec<ApplePurchase>,
    product_id: Option<&str>,
    transaction_id: Option<&str>,
) -> Result<ApplePurchase, V2Error> {
    let product = product_id.filter(|product| !product.is_empty());
    let transaction = transaction_id.filter(|transaction| !transaction.is_empty());
    let mut candidates: Vec<ApplePurchase> = purchases
        .into_iter()
        .filter(|purchase| !purchase.transaction_id.is_empty() && !purchase.product_id.is_empty())
        .filter(|purchase| product.is_none_or(|wanted| purchase.product_id == wanted))
        .collect();
    if let Some(transaction) = transaction {
        candidates.retain(|purchase| purchase.transaction_id == transaction);
    }
    candidates
        .into_iter()
        .max_by_key(|purchase| parse_ms(&purchase.purchase_date_ms))
        .ok_or_else(|| {
            V2Error::failed_precondition(match transaction {
                Some(transaction) => format!("Apple receipt has no transaction {transaction}."),
                None => "Apple receipt contains no purchase.".into(),
            })
        })
}

#[cfg(test)]
mod tests {
    use axum::routing::post;
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

    fn config(prod: &str, sandbox: &str) -> IapConfig {
        IapConfig {
            apple_shared_secret: "shared".into(),
            apple_verify_url: prod.into(),
            apple_sandbox_url: sandbox.into(),
            ..IapConfig::default()
        }
    }

    fn purchase_json(transaction_id: &str, product_id: &str) -> Value {
        json!({
            "status": 0,
            "receipt": {
                "in_app": [{
                    "transaction_id": transaction_id,
                    "product_id": product_id,
                    "purchase_date_ms": "1700000000000",
                    "expires_date_ms": "1800000000000",
                }],
            },
        })
    }

    #[tokio::test]
    async fn a_production_receipt_is_recorded_with_its_transaction() {
        let app = Router::new().route(
            "/verify",
            post(|| async { Json(purchase_json("2001", "gem.small")) }),
        );
        let base = serve(app).await;
        let config = config(&format!("{base}/verify"), &format!("{base}/sandbox"));
        let purchase = verify(&config, "receipt", None, None)
            .await
            .expect("verified");
        assert_eq!(purchase.transaction_id, "2001");
        assert_eq!(purchase.product_id, "gem.small");
        assert_eq!(purchase.environment, 2);
        assert_eq!(
            purchase.purchase_time.expect("purchase time").timestamp(),
            1_700_000_000
        );
        assert!(purchase.expire_time.is_some());
    }

    #[tokio::test]
    async fn a_sandbox_receipt_is_retried_against_the_sandbox_endpoint() {
        async fn prod() -> Json<Value> {
            Json(json!({ "status": 21007 }))
        }
        async fn sandbox() -> Json<Value> {
            Json(purchase_json("3001", "gem.large"))
        }
        let app = Router::new()
            .route("/prod", post(prod))
            .route("/sandbox", post(sandbox));
        let base = serve(app).await;
        let config = config(&format!("{base}/prod"), &format!("{base}/sandbox"));
        let purchase = verify(&config, "receipt", Some("gem.large"), None)
            .await
            .expect("sandbox verified");
        assert_eq!(purchase.environment, 1);
        assert_eq!(purchase.transaction_id, "3001");
    }

    #[tokio::test]
    async fn a_rejected_receipt_never_reaches_the_database() {
        let app = Router::new().route(
            "/verify",
            post(|| async { Json(json!({ "status": 21002 })) }),
        );
        let base = serve(app).await;
        let config = config(&format!("{base}/verify"), &format!("{base}/sandbox"));
        let error = verify(&config, "garbage", None, None)
            .await
            .expect_err("Apple rejected the receipt");
        assert_eq!(error.kind, crate::error::FAILED_PRECONDITION);
        assert!(error.message.contains("21002"));
    }

    #[tokio::test]
    async fn a_product_mismatch_is_refused() {
        let app = Router::new().route(
            "/verify",
            post(|| async { Json(purchase_json("2001", "gem.small")) }),
        );
        let base = serve(app).await;
        let config = config(&format!("{base}/verify"), &format!("{base}/sandbox"));
        let error = verify(&config, "receipt", Some("gem.gold"), None)
            .await
            .expect_err("the receipt is for another product");
        assert_eq!(error.kind, crate::error::FAILED_PRECONDITION);
    }
}
