use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use super::MarketplaceService;
use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::marketplace::{ListItemsRequest, ListingResponse, ListingRow};
use crate::error::V2Error;
use crate::infra::liveops;
use crate::repositories::models::MarketListing;
use crate::services::deps::q;
use crate::services::quest::catalog::counter_name;
use crate::services::quest::reward::payout_uuid;

/// Listings close themselves this many days after they open.
pub(crate) fn market_ttl_days() -> i64 {
    liveops::config().market_ttl_days
}

/// The system holder every escrowed instance sits under while listed.
pub(crate) fn escrow_holder() -> Uuid {
    payout_uuid("marketplace/escrow")
}

pub(crate) fn is_expired(listing: &MarketListing) -> bool {
    Utc::now()
        .signed_duration_since(listing.create_time)
        .num_days()
        >= market_ttl_days()
}

impl MarketplaceService {
    pub async fn list_item(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: ListItemsRequest,
    ) -> Result<Json<ListingResponse>, V2Error> {
        let currency = counter_name(&req.price_currency)?;
        if !req.price.is_finite() || req.price <= 0.0 {
            return Err(V2Error::invalid_argument(
                "A listing price must be a positive amount.",
            ));
        }
        let queries = q(state).await?;
        let instance = queries
            .get_inventory_item(req.instance_id)
            .await?
            .ok_or_else(|| V2Error::not_found("Item not found."))?;
        let escrow = escrow_holder();
        if instance.user_id != auth.uid()
            || !queries
                .hold_item(instance.instance_id, auth.uid(), escrow)
                .await?
                .is_some()
        {
            return Err(V2Error::failed_precondition(
                "Only the owner can list an item.",
            ));
        }
        let now = Utc::now();
        let row = MarketListing {
            id: Uuid::new_v4(),
            seller_id: auth.uid(),
            instance_id: instance.instance_id,
            item_id: instance.item_id.clone(),
            quantity: instance.quantity,
            price_currency: currency.to_string(),
            price: req.price,
            state: "open".to_string(),
            create_time: now,
            update_time: now,
        };
        if let Err(err) = queries.create_listing(&row).await {
            // The hold only exists as long as the listing does.
            let _ = queries
                .move_held_item(row.instance_id, escrow, row.seller_id)
                .await;
            return Err(err.into());
        }
        Ok(Json(ListingResponse {
            success: true,
            listing: ListingRow::from(row),
        }))
    }

    pub async fn cancel(
        &self,
        state: &ApiState,
        auth: &Authed,
        listing_id: Uuid,
    ) -> Result<Json<ListingResponse>, V2Error> {
        let queries = q(state).await?;
        let listing = queries
            .load_listing(listing_id)
            .await?
            .ok_or_else(|| V2Error::not_found("Listing not found."))?;
        if listing.seller_id != auth.uid() {
            return Err(V2Error::permission_denied(
                "Only the seller can cancel a listing.",
            ));
        }
        if listing.state != "open" {
            return Err(V2Error::failed_precondition(
                "This listing is no longer open.",
            ));
        }
        let cancelled = queries
            .flip_listing(listing.id, "cancelled")
            .await?
            .ok_or_else(|| V2Error::failed_precondition("This listing is no longer open."))?;
        if !queries
            .move_held_item(cancelled.instance_id, escrow_holder(), auth.uid())
            .await?
        {
            let _ = queries.reopen_listing(cancelled.id).await;
            return Err(V2Error::unavailable("The held item could not be returned."));
        }
        Ok(Json(ListingResponse {
            success: true,
            listing: ListingRow::from(cancelled),
        }))
    }
}
