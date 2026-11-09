//! Running the rules bound to an event.
//!
//! Rules and hooks answer the same question from two sides: a rule is something
//! an operator writes in the console without shipping code, a hook is something
//! a developer compiles. Both are fired from the same places, rules first, so an
//! operator's "grant 100 gems on purchase" works even when the project has no
//! Wasm module at all — which is most projects.
//!
//! Every rule is isolated: one that fails to evaluate, or whose action cannot be
//! applied, is logged and skipped. Refusing the event because one of several
//! rules was mistyped would turn an operator's typo into an outage.

use uuid::Uuid;

use crate::ApiState;
use crate::error::V2Error;
use crate::repositories::console_mutations::InsertNotificationParams;
use crate::services::deps::q;

use super::action::Action;
use super::eval::evaluate_with_actions;

/// The events a rule may be bound to.
///
/// A closed set, because a rule whose trigger is a typo simply never fires and
/// nothing says so. The console offers these and the publish path refuses
/// anything else.
pub const TRIGGERS: [&str; 6] = [
    "purchase",
    "wallet_change",
    "level_up",
    "match_end",
    "leaderboard_submit",
    "player_create",
];

/// Refuse a trigger the dispatcher does not fire.
///
/// Called at draft time, not at fire time: a rule whose trigger is a typo would
/// otherwise be accepted, published, and silently never run.
pub fn check_trigger(body: &serde_json::Value) -> Result<(), String> {
    let Some(trigger) = body.get("trigger").filter(|value| !value.is_null()) else {
        // A rule with no trigger is the tournament gate, which is fired by key
        // rather than by event. Kept working on purpose.
        return Ok(());
    };
    let named = trigger
        .as_str()
        .map(str::trim)
        .unwrap_or_default()
        .to_owned();
    if TRIGGERS.contains(&named.as_str()) {
        return Ok(());
    }
    Err(format!(
        "trigger '{named}' is not fired by the engine — use one of {} (or leave it out for a tournament rule)",
        TRIGGERS.join(", ")
    ))
}

/// Fire every active rule bound to `trigger`.
///
/// Returns the number of rules that matched, which the caller may log but must
/// not depend on: a game with no rules returns zero and that is not an error.
pub async fn fire(state: &ApiState, trigger: &str, player_id: Uuid) -> usize {
    let queries = match q(state).await {
        Ok(queries) => queries,
        Err(error) => {
            tracing::debug!("[rules] {trigger}: no datastore, no rules ({error:?})");
            return 0;
        }
    };
    let rules = match queries.active_rules_for_trigger(trigger).await {
        Ok(rules) => rules,
        Err(error) => {
            tracing::error!("[rules] {trigger}: rules could not be read: {error}");
            return 0;
        }
    };
    if rules.is_empty() {
        return 0;
    }

    let context = match queries.player_stat_context(player_id).await {
        Ok(context) => context,
        Err(error) => {
            tracing::error!("[rules] {trigger}: no stat context for {player_id}: {error}");
            return 0;
        }
    };

    let mut matched = 0;
    for rule in &rules {
        match evaluate_with_actions(&rule.body, &context) {
            Ok((false, _)) => {}
            Ok((true, actions)) => {
                matched += 1;
                apply(state, player_id, &rule.key, &actions).await;
            }
            Err(error) => {
                tracing::warn!(
                    "[rules] {trigger}: rule '{}' v{} does not evaluate: {error} — skipped",
                    rule.key,
                    rule.version
                );
            }
        }
    }
    matched
}

/// Apply one matched rule's actions.
///
/// Each action is applied on its own. A rule that grants a currency and an item
/// whose definition has since been deleted still delivers the currency, because
/// the alternative is a reward that silently does nothing at all.
async fn apply(state: &ApiState, player_id: Uuid, rule: &str, actions: &[Action]) {
    for action in actions {
        if let Err(error) = apply_one(state, player_id, action).await {
            tracing::warn!(
                "[rules] '{}' action {} failed for {player_id}: {error:?}",
                rule,
                action.kind()
            );
        }
    }
}

async fn apply_one(state: &ApiState, player_id: Uuid, action: &Action) -> Result<(), V2Error> {
    let queries = q(state).await?;
    match action {
        Action::Currency { currency, amount } => {
            let user = queries
                .get_user_by_id(player_id)
                .await
                .map_err(V2Error::from)?;
            let wallet = user.wallet.as_object().cloned().unwrap_or_default();
            let held = wallet
                .get(currency)
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            let next = held + amount;
            if next < 0.0 {
                return Err(V2Error::failed_precondition(format!(
                    "the {currency} balance cannot go negative"
                )));
            }
            let mut changes = serde_json::Map::new();
            changes.insert(currency.clone(), serde_json::json!(amount));
            let mut base = serde_json::Map::new();
            base.insert(currency.clone(), serde_json::json!(held));
            let mut result = serde_json::Map::new();
            result.insert(currency.clone(), serde_json::json!(next));
            queries
                .apply_wallet_change(
                    player_id,
                    &changes,
                    &base,
                    &result,
                    serde_json::json!({ "source": "rule" }),
                    Uuid::new_v4(),
                )
                .await?
                .ok_or_else(|| V2Error::failed_precondition("the balance cannot go negative"))?;
            Ok(())
        }
        Action::Item { item_id, quantity } => {
            let inventory = crate::services::inventory::InventoryService;
            inventory
                .grant_item(
                    state,
                    player_id,
                    crate::dto::inventory::GrantItemRequest {
                        user_id: Some(player_id),
                        item_id: item_id.clone(),
                        quantity: Some(i32::try_from(*quantity).unwrap_or(i32::MAX)),
                        slot_index: None,
                        metadata: None,
                    },
                    Uuid::new_v4(),
                )
                .await?;
            Ok(())
        }
        Action::Notify { subject, body } => {
            queries
                .insert_notification(&InsertNotificationParams {
                    id: Uuid::new_v4(),
                    user_id: player_id,
                    subject: subject.clone(),
                    content: if body.trim().is_empty() {
                        serde_json::json!({})
                    } else {
                        serde_json::json!({ "body": body })
                    },
                    code: 0,
                    sender_id: Uuid::nil(),
                })
                .await?;
            Ok(())
        }
        Action::Badge { badge } => {
            // A badge is not a row that says who holds it: it is a stat reaching
            // a threshold. So granting one means finding the badge's stat and
            // lifting it to the threshold, which is idempotent — a player who
            // already qualifies sees no change, and a second grant cannot
            // double-count anything.
            let definition = queries
                .get_badge_definition(badge)
                .await?
                .ok_or_else(|| V2Error::not_found(format!("badge '{badge}' is not defined")))?;
            let held = queries.get_player_stat(player_id, &definition.stat).await?;
            let reached = held.map(|stat| stat.value).unwrap_or(0);
            if reached < definition.threshold {
                queries
                    .increase_player_stat(
                        player_id,
                        &definition.stat,
                        definition.threshold - reached,
                    )
                    .await?;
            }
            Ok(())
        }
    }
}
