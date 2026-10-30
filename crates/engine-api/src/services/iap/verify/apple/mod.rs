//! Apple `verifyReceipt`: production first, sandbox when Apple answers 21007.

use engine_config::IapConfig;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{parse_ms, VerifiedPurchase};
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
        V2Error::unavailable(format!("Apple verification returned an unreadable body: {err}"))
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
        V2Error::unavailable(format!("Apple verification returned an unexpected body: {err}"))
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
mod tests;
