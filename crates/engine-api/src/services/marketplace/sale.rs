use std::sync::Arc;

use axum::Json;
use uuid::Uuid;

use super::listings::{escrow_holder, is_expired};
use super::{MarketplaceService, transfer};
use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::marketplace::{ListingResponse, ListingRow};
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::models::MarketListing;
use crate::services::deps::q;

impl MarketplaceService {
    pub async fn buy(
        &self,
        state: &ApiState,
        auth: &Authed,
        listing_id: Uuid,
    ) -> Result<Json<ListingResponse>, V2Error> {
        let queries = q(state).await?;
        let listing = load_open(&queries, listing_id, auth.uid()).await?;
        if is_expired(&listing) {
            expire(&queries, &listing).await;
            return Err(V2Error::failed_precondition("This listing has expired."));
        }
        let seed = format!("buy:{}", listing.id);
        transfer(
            &queries,
            auth.uid(),
            &listing.price_currency,
            -listing.price,
            &format!("{seed}/debit"),
        )
        .await?;
        let sold = match queries.flip_listing(listing.id, "sold").await? {
            Some(row) => row,
            None => {
                refund_buyer(&queries, &listing, auth.uid(), &seed).await;
                return Err(V2Error::failed_precondition(
                    "This listing is no longer open.",
                ));
            }
        };
        if !queries
            .move_held_item(sold.instance_id, escrow_holder(), auth.uid())
            .await?
        {
            reverse_sale(&queries, &sold, auth.uid(), &seed).await;
            return Err(V2Error::unavailable(
                "The purchased item could not be delivered.",
            ));
        }
        if let Err(err) = transfer(
            &queries,
            sold.seller_id,
            &sold.price_currency,
            sold.price,
            &format!("{seed}/credit"),
        )
        .await
        {
            reverse_sale(&queries, &sold, auth.uid(), &seed).await;
            return Err(err);
        }
        Ok(Json(ListingResponse {
            success: true,
            listing: ListingRow::from(sold),
        }))
    }
}

async fn load_open(
    queries: &Arc<Queries>,
    listing_id: Uuid,
    buyer: Uuid,
) -> Result<MarketListing, V2Error> {
    let listing = queries
        .load_listing(listing_id)
        .await?
        .ok_or_else(|| V2Error::not_found("Listing not found."))?;
    if listing.seller_id == buyer {
        return Err(V2Error::invalid_argument(
            "A seller cannot buy their own listing.",
        ));
    }
    if listing.state != "open" {
        return Err(V2Error::failed_precondition(
            "This listing is no longer open.",
        ));
    }
    Ok(listing)
}

async fn expire(queries: &Arc<Queries>, listing: &MarketListing) {
    if queries.flip_listing(listing.id, "expired").await.is_ok() {
        let _ = queries
            .move_held_item(listing.instance_id, escrow_holder(), listing.seller_id)
            .await;
    }
}

async fn refund_buyer(queries: &Arc<Queries>, listing: &MarketListing, buyer: Uuid, seed: &str) {
    let _ = transfer(
        queries,
        buyer,
        &listing.price_currency,
        listing.price,
        &format!("{seed}/refund"),
    )
    .await;
}

/// Undoes a sale the buyer was charged for: the item goes back on escrow, the
/// listing reopens, and the buyer's money returns.
async fn reverse_sale(
    queries: &Arc<Queries>,
    listing: &MarketListing,
    buyer: Uuid,
    seed: &str,
) {
    let _ = queries
        .move_held_item(listing.instance_id, buyer, escrow_holder())
        .await;
    let _ = queries.reopen_listing(listing.id).await;
    refund_buyer(queries, listing, buyer, seed).await;
}
