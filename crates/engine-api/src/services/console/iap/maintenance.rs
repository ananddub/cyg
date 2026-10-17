//! `DELETE /v2/console/iap/{vendor}/reset` and
//! `DELETE /v2/console/subscription/{product_id}` — the two console writes that
//! undo IAP state.
//!
//! A vendor's stored configuration lives in the `setting` table under
//! `iap.{vendor}` (the console settings surface is the only credential store
//! this crate has); a reset deletes that key. A subscription delete removes
//! every subscription row for one product id: the schema links purchases to
//! subscriptions by nothing at all — no foreign key, no shared id column — so
//! purchase rows for the same product survive by design. The delete statement
//! is hand-written and mirrored in `db/manual/iap.sql`.

use axum::Json;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::error::V2Error;
use crate::ApiState;

use super::Vendor;

/// The setting key a vendor's stored configuration lives under.
pub(super) fn config_key(vendor: &str) -> String {
    format!("iap.{vendor}")
}

impl super::super::ConsoleService {
    pub async fn reset_vendor(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        vendor_name: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let vendor = Vendor::parse(vendor_name).ok_or_else(|| {
            V2Error::invalid_argument(format!("Unknown IAP vendor: {vendor_name}"))
        })?;
        let key = config_key(vendor_name);
        if super::super::queries(state)?
            .delete_setting(&key)
            .await?
            .rows_affected()
            == 0
        {
            return Err(V2Error::not_found(format!(
                "No stored {} IAP configuration.",
                vendor.label()
            )));
        }
        super::super::audit(
            state,
            claims,
            "iap.vendor.reset",
            "iap",
            None,
            &format!("Cleared stored {vendor_name} IAP configuration."),
        )
        .await;
        Ok(Json(Empty {}))
    }

    pub async fn delete_subscription_product(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        product_id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let product_id = product_id.trim();
        if product_id.is_empty() {
            return Err(V2Error::invalid_argument("A product id is required."));
        }
        let pool = super::super::pool(state)?;
        let deleted = sqlx::query("DELETE FROM subscription WHERE product_id = $1")
            .bind(product_id)
            .execute(pool)
            .await?
            .rows_affected();
        if deleted == 0 {
            return Err(V2Error::not_found("Subscription not found for this product."));
        }
        super::super::audit(
            state,
            claims,
            "subscription.delete",
            "subscription",
            None,
            &format!("Deleted {deleted} subscription(s) for product {product_id}."),
        )
        .await;
        Ok(Json(Empty {}))
    }
}
