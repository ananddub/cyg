//! Google Play: service-account token, then the Android Publisher lookup.
//!
//! A product lookup needs the product id; a subscription lookup does not. When
//! a product lookup reports "not found" the token is retried as a subscription,
//! which is how a store receipt with no known product still verifies.

mod token;

use engine_config::IapConfig;
use serde_json::Value;

use super::{parse_ms, parse_rfc3339, VerifiedPurchase};
use crate::error::V2Error;
use crate::utils::iap::hex_id;

pub(super) async fn verify(
    config: &IapConfig,
    credential: &str,
    product_id: Option<&str>,
) -> Result<VerifiedPurchase, V2Error> {
    let access = token::access_token(config).await?;
    if let Some(product) = product_id.filter(|product| !product.is_empty()) {
        let url = endpoint(
            config,
            &[
                "applications",
                &config.google_package_name,
                "purchases",
                "products",
                product,
                "tokens",
                credential,
            ],
        )?;
        if let Some(value) = lookup(url, &access).await? {
            return product_purchase(value, product);
        }
    }
    let url = endpoint(
        config,
        &[
            "applications",
            &config.google_package_name,
            "purchases",
            "subscriptionsv2",
            "tokens",
            credential,
        ],
    )?;
    let value = lookup(url, &access)
        .await?
        .ok_or_else(|| V2Error::failed_precondition("Google has no purchase for this token."))?;
    subscription_purchase(value, credential)
}

fn endpoint(config: &IapConfig, tail: &[&str]) -> Result<reqwest::Url, V2Error> {
    let base = format!("{}/", config.google_api_base.trim_end_matches('/'));
    let mut url = reqwest::Url::parse(&base)
        .map_err(|err| V2Error::unavailable(format!("[control.iap] google_api_base is invalid: {err}")))?;
    {
        let mut segments = url.path_segments_mut().map_err(|_| {
            V2Error::unavailable("[control.iap] google_api_base cannot be a base URL.")
        })?;
        segments.pop_if_empty();
        for segment in tail {
            segments.push(segment);
        }
    }
    Ok(url)
}

async fn lookup(url: reqwest::Url, access: &str) -> Result<Option<Value>, V2Error> {
    let response = super::client()
        .get(url)
        .bearer_auth(access)
        .send()
        .await
        .map_err(|err| V2Error::unavailable(format!("Google purchase lookup failed: {err}")))?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::BAD_REQUEST {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(V2Error::unavailable(format!(
            "Google purchase lookup answered {status}."
        )));
    }
    let value = response.json().await.map_err(|err| {
        V2Error::unavailable(format!("Google purchase lookup returned an unreadable body: {err}"))
    })?;
    Ok(Some(value))
}

fn product_purchase(value: Value, product: &str) -> Result<VerifiedPurchase, V2Error> {
    let state = value
        .get("purchaseState")
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    if state != 0 {
        return Err(V2Error::failed_precondition(format!(
            "Google reports purchase state {state}."
        )));
    }
    let transaction_id = value
        .get("orderId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| V2Error::failed_precondition("Google purchase has no order id."))?
        .to_owned();
    Ok(VerifiedPurchase {
        transaction_id,
        product_id: product.to_owned(),
        environment: 2,
        purchase_time: value
            .get("purchaseTimeMillis")
            .and_then(Value::as_str)
            .and_then(parse_ms),
        expire_time: None,
        raw: value,
    })
}

fn subscription_purchase(value: Value, credential: &str) -> Result<VerifiedPurchase, V2Error> {
    let state = value
        .get("subscriptionState")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(
        state,
        "SUBSCRIPTION_STATE_ACTIVE" | "SUBSCRIPTION_STATE_IN_GRACE_PERIOD"
    ) {
        return Err(V2Error::failed_precondition(format!(
            "Google reports subscription state {state}."
        )));
    }
    let line = value
        .get("lineItems")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .cloned()
        .unwrap_or(Value::Null);
    let product_id = line
        .get("productId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| V2Error::failed_precondition("Google subscription has no product id."))?
        .to_owned();
    let transaction_id = value
        .get("latestOrderId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| hex_id(credential));
    Ok(VerifiedPurchase {
        transaction_id,
        product_id,
        environment: 2,
        purchase_time: value
            .get("startTime")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339),
        expire_time: line
            .get("expiryTime")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339),
        raw: value,
    })
}

#[cfg(test)]
mod tests;
