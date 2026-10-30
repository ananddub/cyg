use axum::Json;
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use super::{MarketplaceService, transfer};
use crate::ApiState;
use crate::dto::marketplace::{WagerCreateRequest, WagerResponse, WagerRow, WagerSettleRequest};
use crate::error::V2Error;
use crate::repositories::models::Wager;
use crate::services::deps::q;
use crate::services::quest::catalog::counter_name;

/// The largest stake one account may put behind a wager.
const MAX_STAKE: f64 = 1_000_000.0;
const MAX_BETTORS: usize = 8;

impl MarketplaceService {
    pub async fn create_wager(
        &self,
        state: &ApiState,
        req: WagerCreateRequest,
    ) -> Result<Json<WagerResponse>, V2Error> {
        let room = req.room.trim();
        if room.is_empty() || room.len() > 128 {
            return Err(V2Error::invalid_argument(
                "A wager needs a room name of at most 128 characters.",
            ));
        }
        if req.players.len() < 2 || req.players.len() > MAX_BETTORS {
            return Err(V2Error::invalid_argument(format!(
                "A wager takes between 2 and {MAX_BETTORS} bettors."
            )));
        }
        let unique = req
            .players
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        if unique.len() != req.players.len() {
            return Err(V2Error::invalid_argument(
                "A wager bettor must appear only once.",
            ));
        }
        let currency = counter_name(&req.currency)?;
        if !req.amount_each.is_finite() || req.amount_each <= 0.0 || req.amount_each > MAX_STAKE {
            return Err(V2Error::invalid_argument(format!(
                "A wager stake must be above 0 and at most {MAX_STAKE}."
            )));
        }
        let queries = q(state).await?;
        let id = Uuid::new_v4();
        let mut debited = Vec::new();
        for player in &req.players {
            let seed = format!("wager:{id}/debit/{player}");
            if let Err(err) = transfer(&queries, *player, currency, -req.amount_each, &seed).await {
                // Nobody's stake may sit held while the wager does not exist.
                for done in &debited {
                    let _ = transfer(
                        &queries,
                        *done,
                        currency,
                        req.amount_each,
                        &format!("wager:{id}/undebit/{done}"),
                    )
                    .await;
                }
                return Err(err);
            }
            debited.push(*player);
        }
        let now = Utc::now();
        let row = Wager {
            id,
            room: room.to_string(),
            players: req.players,
            currency: currency.to_string(),
            amount_each: req.amount_each,
            state: "held".to_string(),
            outcome: json!({}),
            create_time: now,
            update_time: now,
        };
        if let Err(err) = queries.create_wager(&row).await {
            for player in &debited {
                let _ = transfer(
                    &queries,
                    *player,
                    currency,
                    req.amount_each,
                    &format!("wager:{id}/undebit/{player}"),
                )
                .await;
            }
            return Err(err.into());
        }
        Ok(Json(WagerResponse {
            success: true,
            wager: WagerRow::from(row),
        }))
    }

    pub async fn settle_wager(
        &self,
        state: &ApiState,
        wager_id: Uuid,
        req: WagerSettleRequest,
    ) -> Result<Json<WagerResponse>, V2Error> {
        let queries = q(state).await?;
        let wager = closed_once(&queries, wager_id).await?;
        if !wager.players.contains(&req.winner) {
            return Err(V2Error::invalid_argument(
                "The winner must be one of the bettors.",
            ));
        }
        let settled = queries
            .flip_wager(wager.id, "settled", &req.data)
            .await?
            .ok_or_else(|| V2Error::failed_precondition("This wager is already closed."))?;
        let pool = settled.amount_each * settled.players.len() as f64;
        let seed = format!("wager:{wager_id}/credit/{}", req.winner);
        if let Err(err) = transfer(&queries, req.winner, &settled.currency, pool, &seed).await {
            let _ = queries.reopen_wager(wager_id).await;
            return Err(err);
        }
        crate::services::webhook::emit(
            state,
            "marketplace.trade.settled",
            &format!("{wager_id}:settled"),
            serde_json::json!({
                "kind": "wager.settled",
                "wager_id": settled.id,
                "room": settled.room,
                "winner": req.winner,
                "players": settled.players,
                "currency": settled.currency,
                "pool": pool,
            }),
        )
        .await;
        Ok(Json(WagerResponse {
            success: true,
            wager: WagerRow::from(settled),
        }))
    }
}

/// Reads a wager and refuses up front when it is missing or already closed;
/// the guarded flip stays as the real single-flight backstop.
pub(super) async fn closed_once(
    queries: &std::sync::Arc<crate::repositories::db::Queries>,
    wager_id: Uuid,
) -> Result<Wager, V2Error> {
    let wager = queries
        .load_wager(wager_id)
        .await?
        .ok_or_else(|| V2Error::not_found("Wager not found."))?;
    if wager.state != "held" {
        return Err(V2Error::failed_precondition(
            "This wager is already closed.",
        ));
    }
    Ok(wager)
}
