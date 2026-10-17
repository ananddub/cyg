//! `POST /v2/console/iap/{vendor}/validate` — an operator verifies one vendor
//! credential server-side and gets the parsed entity back.
//!
//! The reply shapes mirror the reference server: a purchase validation answers
//! `api.ValidatePurchaseResponse` (`{"validated_purchases": [ … ]}`) and a
//! subscription validation answers `api.ValidateSubscriptionResponse`
//! (`{"subscription": …}`). Like the reference, `persist` defaults to true; an
//! unpersisted validation touches no database at all.

use axum::Json;
use axum::response::{IntoResponse, Response};
use chrono::Utc;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::console::iap::{
    ConsoleValidateRequest, ValidatedPurchaseReply, ValidatedSubscriptionReply,
};
use crate::dto::iap::{PurchaseResponse, SubscriptionResponse};
use crate::error::V2Error;
use crate::repositories::iap::{InsertPurchaseParams, InsertSubscriptionParams};
use crate::utils::iap::{environment_of, epoch, hex_id};
use crate::ApiState;

use super::Vendor;

/// What the validation wrote is what it replies with — one of the two mirror
/// shapes, decided by the request's `kind`.
pub enum ValidatedReply {
    Purchase(Json<ValidatedPurchaseReply>),
    Subscription(Json<ValidatedSubscriptionReply>),
}

impl IntoResponse for ValidatedReply {
    fn into_response(self) -> Response {
        match self {
            ValidatedReply::Purchase(reply) => reply.into_response(),
            ValidatedReply::Subscription(reply) => reply.into_response(),
        }
    }
}

impl super::super::ConsoleService {
    pub async fn validate_vendor(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        vendor_name: &str,
        body: ConsoleValidateRequest,
    ) -> Result<ValidatedReply, V2Error> {
        claims.require_write()?;
        let vendor = Vendor::parse(vendor_name)
            .ok_or_else(|| V2Error::invalid_argument(format!("Unknown IAP vendor: {vendor_name}")))?;
        let subscription = match body.kind.as_deref().filter(|k| !k.is_empty()) {
            None | Some("purchase") => false,
            Some("subscription") => true,
            Some(_) => {
                return Err(V2Error::invalid_argument(
                    "Validation kind must be \"purchase\" or \"subscription\".",
                ));
            }
        };
        if subscription && !vendor.supports_subscriptions() {
            return Err(V2Error::invalid_argument(format!(
                "{} subscriptions are not supported.",
                vendor.label()
            )));
        }
        let credential = body.credential();
        if credential.is_empty() {
            return Err(V2Error::invalid_argument(
                "receipt or purchase token is required",
            ));
        }

        if !body.persists() {
            let id = hex_id(credential);
            let product = body.product_id.clone().unwrap_or_default();
            return Ok(if subscription {
                ValidatedReply::Subscription(Json(ValidatedSubscriptionReply {
                    subscription: SubscriptionResponse::parsed(&product, &id),
                }))
            } else {
                ValidatedReply::Purchase(Json(ValidatedPurchaseReply {
                    validated_purchases: vec![PurchaseResponse::parsed(&product, &id)],
                }))
            });
        }

        let raw_id = body.user_id.as_deref().unwrap_or_default();
        if raw_id.is_empty() {
            return Err(V2Error::invalid_argument(
                "Persisting a validation needs a user id.",
            ));
        }
        let uid = super::super::id(raw_id, "account")?;
        let queries = super::super::queries(state)?;
        queries.get_user_by_id(uid).await?;
        let product_id = body.product_id.clone().unwrap_or_default();
        let transaction_id = hex_id(credential);
        let raw = serde_json::Value::String(credential.to_owned());
        if subscription {
            let row = queries
                .insert_subscription(&InsertSubscriptionParams {
                    original_transaction_id: transaction_id,
                    user_id: uid,
                    store: vendor.store(),
                    environment: environment_of(credential, vendor.store()),
                    product_id,
                    purchase_time: Utc::now(),
                    expire_time: epoch(),
                    refund_time: epoch(),
                    raw_response: raw,
                })
                .await?;
            super::super::audit(
                state,
                claims,
                "iap.validate",
                "iap",
                Some(uid),
                &format!("Validated a {} subscription.", vendor.label()),
            )
            .await;
            Ok(ValidatedReply::Subscription(Json(ValidatedSubscriptionReply {
                subscription: SubscriptionResponse::from(&row),
            })))
        } else {
            let row = queries
                .insert_purchase(&InsertPurchaseParams {
                    transaction_id,
                    user_id: uid,
                    store: vendor.store(),
                    environment: environment_of(credential, vendor.store()),
                    product_id,
                    purchase_time: Utc::now(),
                    refund_time: epoch(),
                    raw_response: raw,
                })
                .await?;
            super::super::audit(
                state,
                claims,
                "iap.validate",
                "iap",
                Some(uid),
                &format!("Validated a {} purchase.", vendor.label()),
            )
            .await;
            Ok(ValidatedReply::Purchase(Json(ValidatedPurchaseReply {
                validated_purchases: vec![PurchaseResponse::from(&row)],
            })))
        }
    }
}
