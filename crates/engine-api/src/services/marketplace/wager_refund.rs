use axum::Json;
use serde_json::json;
use uuid::Uuid;

use super::wagers::closed_once;
use super::{MarketplaceService, transfer};
use crate::ApiState;
use crate::dto::marketplace::{WagerResponse, WagerRow};
use crate::error::V2Error;
use crate::services::deps::q;

impl MarketplaceService {
    pub async fn refund_wager(
        &self,
        state: &ApiState,
        wager_id: Uuid,
    ) -> Result<Json<WagerResponse>, V2Error> {
        let queries = q(state).await?;
        let wager = closed_once(&queries, wager_id).await?;
        let refunded = queries
            .flip_wager(wager.id, "refunded", &json!({}))
            .await?
            .ok_or_else(|| V2Error::failed_precondition("This wager is already closed."))?;
        let mut failure = None;
        for player in &refunded.players {
            let seed = format!("wager:{wager_id}/refund/{player}");
            if let Err(err) = transfer(
                &queries,
                *player,
                &refunded.currency,
                refunded.amount_each,
                &seed,
            )
            .await
                && failure.is_none()
            {
                failure = Some(err);
            }
        }
        if let Some(err) = failure {
            return Err(err);
        }
        crate::services::webhook::emit(
            state,
            "marketplace.trade.settled",
            &format!("{wager_id}:refunded"),
            serde_json::json!({
                "kind": "wager.refunded",
                "wager_id": refunded.id,
                "room": refunded.room,
                "players": refunded.players,
                "currency": refunded.currency,
                "amount_each": refunded.amount_each,
            }),
        )
        .await;
        Ok(Json(WagerResponse {
            success: true,
            wager: WagerRow::from(refunded),
        }))
    }
}
