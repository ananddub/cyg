//! Applying what a hook proposed.
//!
//! A hook never writes. It answers an event with proposals, and this module
//! turns those proposals into the same service calls the REST API makes — so a
//! grant that arrives through a hook is subject to the same balance floor, the
//! same item definition check and the same audit trail as one that arrives from
//! the console. A customer cannot use a hook to walk around a rule the API
//! enforces, which is the whole difference from Nakama's `nk.walletUpdate()`.
//!
//! A hook that errors, or that answers with something this module cannot apply,
//! is reported and ignored: the event it accompanied has already happened in the
//! outside world (apple charged the card), so refusing it would leave the player
//! paid-up and unrewarded.

use std::collections::BTreeMap;

use uuid::Uuid;

use crate::ApiState;
use crate::error::V2Error;
use crate::repositories::console_mutations::InsertNotificationParams;
use crate::services::deps::q;
use engine_hooks::{HookEvent, HookInput, HookOutput};

/// Run a hook for an event and apply whatever it proposes.
///
/// Returns `Err` only when the hook asked for the event to be refused. Every
/// other failure — a trap, a bad proposal, a missing item — is logged and
/// treated as "no hook answered", because a broken hook must not become a
/// broken purchase.
pub async fn fire(
    state: &ApiState,
    event: HookEvent,
    player_id: Uuid,
    payload: serde_json::Value,
) -> Result<(), V2Error> {
    let Some(runner) = state.hooks.as_ref() else {
        return Ok(());
    };
    let input = HookInput {
        event,
        player_id: player_id.to_string(),
        payload,
    };
    let output = match runner.run(&input) {
        Ok(Some(output)) => output,
        // The module does not answer this event. Not an error.
        Ok(None) => return Ok(()),
        Err(e) => {
            tracing::error!(
                "[hooks] {} hook failed for {player_id}: {e:?} — event continues without it",
                event.as_str()
            );
            return Ok(());
        }
    };

    if let Some(reason) = output.reject.as_ref().filter(|r| !r.trim().is_empty()) {
        return Err(V2Error::failed_precondition(format!(
            "Refused by game logic: {reason}"
        )));
    }
    apply(state, player_id, &output).await;
    Ok(())
}

/// Apply proposals. Failures are logged per proposal so one bad grant does not
/// discard the rest.
async fn apply(state: &ApiState, player_id: Uuid, output: &HookOutput) {
    grant_currency(state, player_id, output).await;
    grant_items(state, player_id, output).await;
    notify(state, player_id, output).await;
}

async fn grant_currency(state: &ApiState, player_id: Uuid, output: &HookOutput) {
    if output.grant_currency.is_empty() {
        return;
    }
    // Sum first: two grants of the same currency from one hook are one change,
    // and applying them separately would read the balance twice and lose the
    // first, since each read starts from the stored row.
    let mut deltas: BTreeMap<&str, i64> = BTreeMap::new();
    for grant in &output.grant_currency {
        *deltas.entry(grant.currency.as_str()).or_default() += grant.amount;
    }
    let deltas = deltas.into_iter().filter(|(_, amount)| *amount != 0);

    let queries = match q(state).await {
        Ok(q) => q,
        Err(e) => {
            tracing::error!("[hooks] currency grant skipped for {player_id}: {e:?}");
            return;
        }
    };
    let user = match queries.get_user_by_id(player_id).await {
        Ok(user) => user,
        Err(e) => {
            tracing::error!("[hooks] currency grant skipped for {player_id}: {e:?}");
            return;
        }
    };
    let wallet = user.wallet.as_object().cloned().unwrap_or_default();
    let mut changes = serde_json::Map::new();
    let mut base = serde_json::Map::new();
    let mut result = serde_json::Map::new();
    for (currency, amount) in deltas {
        let held = wallet
            .get(currency)
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let next = held + amount as f64;
        if next < 0.0 {
            tracing::warn!(
                "[hooks] {currency} grant of {amount} for {player_id} would go negative — skipped"
            );
            continue;
        }
        changes.insert(currency.to_owned(), serde_json::json!(amount));
        base.insert(currency.to_owned(), serde_json::json!(held));
        result.insert(currency.to_owned(), serde_json::json!(next));
    }
    if changes.is_empty() {
        return;
    }
    let reason = output
        .grant_currency
        .iter()
        .map(|g| g.reason.as_str())
        .find(|r| !r.trim().is_empty())
        .unwrap_or("hook");
    let metadata = serde_json::json!({ "source": "hook", "reason": reason });
    match queries
        .apply_wallet_change(
            player_id,
            &changes,
            &base,
            &result,
            metadata,
            Uuid::new_v4(),
        )
        .await
    {
        Ok(Some(_)) => {}
        Ok(None) => tracing::warn!(
            "[hooks] currency grant for {player_id} was rejected by the balance floor"
        ),
        Err(e) => tracing::error!("[hooks] currency grant failed for {player_id}: {e:?}"),
    }
}

async fn grant_items(state: &ApiState, player_id: Uuid, output: &HookOutput) {
    if output.grant_items.is_empty() {
        return;
    }
    let inventory = crate::services::inventory::InventoryService;
    for grant in &output.grant_items {
        let req = crate::dto::inventory::GrantItemRequest {
            user_id: Some(player_id),
            item_id: grant.item_id.clone(),
            quantity: Some(i32::try_from(grant.quantity).unwrap_or(i32::MAX)),
            slot_index: None,
            metadata: None,
        };
        if let Err(e) = inventory
            .grant_item(state, player_id, req, Uuid::new_v4())
            .await
        {
            tracing::warn!(
                "[hooks] item grant {}x{} for {player_id} failed: {e:?}",
                grant.item_id,
                grant.quantity
            );
        }
    }
}

async fn notify(state: &ApiState, player_id: Uuid, output: &HookOutput) {
    if output.notify.is_empty() {
        return;
    }
    let queries = match q(state).await {
        Ok(q) => q,
        Err(e) => {
            tracing::error!("[hooks] notifications skipped for {player_id}: {e:?}");
            return;
        }
    };
    for note in &output.notify {
        let target = if note.player_id.trim().is_empty() {
            player_id
        } else {
            match Uuid::parse_str(note.player_id.trim()) {
                Ok(id) => id,
                Err(_) => {
                    tracing::warn!(
                        "[hooks] notification skipped: {:?} is not a user id",
                        note.player_id
                    );
                    continue;
                }
            }
        };
        let params = InsertNotificationParams {
            id: Uuid::new_v4(),
            user_id: target,
            subject: note.subject.clone(),
            // The hook contract carries a body as text; the inbox stores a JSON
            // document, so it is wrapped rather than parsed — a hook that wants
            // structure in there is better served by a payload contract change
            // than by guessing whether its text is JSON.
            content: if note.body.trim().is_empty() {
                serde_json::json!({})
            } else {
                serde_json::json!({ "body": note.body })
            },
            code: 0,
            sender_id: Uuid::nil(),
        };
        if let Err(e) = queries.insert_notification(&params).await {
            tracing::warn!("[hooks] notification to {target} failed: {e}");
        }
    }
}
