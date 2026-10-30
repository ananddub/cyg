use std::sync::Arc;
use uuid::Uuid;

use crate::error::V2Error;
use crate::repositories::db::Queries;

use super::reward::{Reward, payout_uuid};

/// Pays a reward through the same guarded writes the economy endpoints use: the
/// wallet change is a compare-and-set with its own ledger row, and an item grant
/// reuses a stack when one has room. Row ids come from `seed`, so a payout that
/// is retried touches the same rows rather than minting new ones.
pub(crate) async fn pay(
    queries: &Arc<Queries>,
    user_id: Uuid,
    reward: &Reward,
    seed: &str,
) -> Result<serde_json::Value, V2Error> {
    if !reward.currency.is_empty() {
        credit(queries, user_id, reward, seed).await?;
    }
    for (index, (item_id, quantity)) in reward.items.iter().enumerate() {
        grant(
            queries,
            user_id,
            item_id,
            *quantity,
            &format!("{seed}/item/{index}"),
        )
        .await?;
    }
    Ok(reward.summary())
}

async fn credit(
    queries: &Arc<Queries>,
    user_id: Uuid,
    reward: &Reward,
    seed: &str,
) -> Result<(), V2Error> {
    let user = queries.get_user_by_id(user_id).await?;
    let wallet = user.wallet.as_object().cloned().unwrap_or_default();
    let mut changes = serde_json::Map::new();
    let mut base = serde_json::Map::new();
    let mut result = serde_json::Map::new();
    for (currency, amount) in &reward.currency {
        let held = wallet
            .get(currency)
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        changes.insert(currency.clone(), serde_json::json!(amount));
        base.insert(currency.clone(), serde_json::json!(held));
        result.insert(currency.clone(), serde_json::json!(held + amount));
    }
    queries
        .apply_wallet_change(
            user_id,
            &changes,
            &base,
            &result,
            serde_json::json!({ "source": seed }),
            payout_uuid(&format!("{seed}/currency")),
        )
        .await?
        .ok_or_else(|| {
            V2Error::internal("The reward was granted but the account could not be read back.")
        })?;
    Ok(())
}

async fn grant(
    queries: &Arc<Queries>,
    user_id: Uuid,
    item_id: &str,
    quantity: i32,
    seed: &str,
) -> Result<(), V2Error> {
    let definition = queries
        .get_item_definition(item_id)
        .await?
        .ok_or_else(|| V2Error::not_found(format!("item definition '{item_id}' not found")))?;
    let quantity = quantity.min(definition.max_stack.max(1));
    if definition.max_stack > 1
        && let Some(stack) = queries
            .find_inventory_stack(user_id, item_id, quantity, definition.max_stack)
            .await?
    {
        queries
            .bump_inventory_quantity(stack.instance_id, quantity)
            .await?;
        return Ok(());
    }
    queries
        .grant_inventory_item(
            payout_uuid(seed),
            user_id,
            item_id,
            quantity,
            -1,
            &serde_json::json!({}),
        )
        .await?;
    Ok(())
}
