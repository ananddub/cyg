//! Store-side receipt verification.
//!
//! A purchase is recorded only after the store's own API confirms it. Apple
//! receipts go to `verifyReceipt`, Google tokens to the Android Publisher API.
//! A store with no verifier answers 503 unless `[control.iap] allow_unverified`
//! is set, which is a local-development escape hatch and never a production
//! setting.

mod apple;
mod google;

use std::sync::OnceLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use engine_config::IapConfig;

use crate::error::V2Error;
use crate::utils::iap::hex_id;

pub(crate) const APPLE: i16 = 0;
pub(crate) const GOOGLE: i16 = 1;

/// What a store's own API confirmed about a purchase.
#[derive(Debug, Clone)]
pub(crate) struct VerifiedPurchase {
    pub transaction_id: String,
    pub product_id: String,
    /// 1 = sandbox, 2 = production, 0 = unknown (unverified development mode).
    pub environment: i16,
    pub purchase_time: Option<DateTime<Utc>>,
    pub expire_time: Option<DateTime<Utc>>,
    pub raw: serde_json::Value,
}

/// Confirm a receipt or purchase token with the store that issued it.
pub(crate) async fn verify_purchase(
    config: &IapConfig,
    store: i16,
    credential: &str,
    product_id: Option<&str>,
    transaction_id: Option<&str>,
) -> Result<VerifiedPurchase, V2Error> {
    match store {
        APPLE if config.apple_configured() => {
            apple::verify(config, credential, product_id, transaction_id).await
        }
        GOOGLE if config.google_configured() => {
            google::verify(config, credential, product_id).await
        }
        _ if config.allow_unverified => {
            tracing::warn!("[iap] recording an unverified purchase for store {store}");
            Ok(unverified(store, credential, product_id, transaction_id))
        }
        APPLE => Err(V2Error::unavailable(
            "Apple purchase verification is not configured, set [control.iap] apple_shared_secret.",
        )),
        GOOGLE => Err(V2Error::unavailable(
            "Google purchase verification is not configured, set [control.iap] google_service_account and google_package_name.",
        )),
        other => Err(V2Error::unavailable(format!(
            "Purchase verification is not implemented for store {other}."
        ))),
    }
}

fn unverified(
    _store: i16,
    credential: &str,
    product_id: Option<&str>,
    transaction_id: Option<&str>,
) -> VerifiedPurchase {
    VerifiedPurchase {
        transaction_id: transaction_id
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| hex_id(credential)),
        product_id: product_id.unwrap_or_default().to_owned(),
        environment: 0,
        purchase_time: None,
        expire_time: None,
        raw: serde_json::Value::String(credential.to_owned()),
    }
}

fn client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .unwrap_or_default()
        })
        .clone()
}

fn parse_ms(value: &str) -> Option<DateTime<Utc>> {
    value.parse::<i64>().ok().and_then(|ms| {
        if ms <= 0 {
            return None;
        }
        DateTime::from_timestamp_millis(ms)
    })
}

fn parse_rfc3339(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unverified_purchase_keeps_the_client_claim() {
        let purchase = unverified(GOOGLE, "token-123", Some("gem.small"), None);
        assert_eq!(purchase.transaction_id, hex_id("token-123"));
        assert_eq!(purchase.product_id, "gem.small");
        assert_eq!(purchase.environment, 0);
        assert!(purchase.purchase_time.is_none());
    }

    #[test]
    fn an_explicit_transaction_id_wins_over_the_hash() {
        let purchase = unverified(APPLE, "receipt", None, Some("2000000123"));
        assert_eq!(purchase.transaction_id, "2000000123");
    }

    #[tokio::test]
    async fn an_unconfigured_store_refuses_instead_of_trusting_the_client() {
        let config = IapConfig::default();
        let error = verify_purchase(&config, APPLE, "receipt", None, None)
            .await
            .expect_err("no verifier and no escape hatch");
        assert!(error.message.contains("apple_shared_secret"));
        let error = verify_purchase(&config, GOOGLE, "token", None, None)
            .await
            .expect_err("no verifier and no escape hatch");
        assert!(error.message.contains("google_service_account"));
        let error = verify_purchase(&config, 4, "voucher", None, None)
            .await
            .expect_err("no verifier for vendor stores");
        assert!(error.message.contains("store 4"));
    }

    #[tokio::test]
    async fn the_development_escape_hatch_records_unknown_environment() {
        let config = IapConfig {
            allow_unverified: true,
            ..IapConfig::default()
        };
        let purchase = verify_purchase(&config, APPLE, "receipt", Some("gem"), None)
            .await
            .expect("unverified fallback");
        assert_eq!(purchase.environment, 0);
    }
}
