use axum::routing::post;
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
    let purchase = verify(&config, "receipt", None, None).await.expect("verified");
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
