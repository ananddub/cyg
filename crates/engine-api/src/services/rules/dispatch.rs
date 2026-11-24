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
use crate::services::plane::Plane;

use std::sync::Arc;

use uuid::Uuid;

use crate::error::V2Error;
use crate::repositories::console_mutations::InsertNotificationParams;

use super::action::{Action, parse_limits};
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

/// A custom trigger is the name of an event the game itself sends: anything in
/// the game's `[gameplay] record_events` reaches the pump, which fires rules
/// bound to it. The shape is constrained, not the vocabulary — a lowercase
/// snake_case name can be wired up, while "Boss Died!" would never match what
/// the mirror carries and would silently never run.
pub fn is_custom_trigger(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    name.len() <= 64 && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Refuse a trigger the dispatcher does not fire.
///
/// Called at draft time, not at fire time: a rule whose trigger is a typo would
/// otherwise be accepted, published, and silently never run. The six built-ins
/// are fired by the engine itself; any other lowercase snake_case name is a
/// user-defined trigger the game mirrors through the bus.
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
    if is_custom_trigger(&named) {
        return Ok(());
    }
    Err(format!(
        "trigger '{named}' is not fired by the engine — use one of {}, or a lowercase snake_case event the game sends (record_events), or leave it out for a tournament rule",
        TRIGGERS.join(", ")
    ))
}

/// Fire every active rule bound to `trigger`.
///
/// Returns the number of rules that matched, which the caller may log but must
/// not depend on: a game with no rules returns zero and that is not an error.
pub async fn fire(plane: &Plane, trigger: &str, player_id: Uuid) -> usize {
    let queries = match plane.q().await {
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

    let stats = match queries.player_stat_context(player_id).await {
        Ok(context) => context,
        Err(error) => {
            tracing::error!("[rules] {trigger}: no stat context for {player_id}: {error}");
            return 0;
        }
    };

    let mut matched = 0;
    for rule in &rules {
        // A rule's `variables` are its own named values — the operator's
        // "$min_kills" lives here, so a comparison can name the variable
        // instead of hard-coding the number. They shadow stats: the value the
        // operator typed for this rule is the one this rule compares against.
        // Merged once: the same merge used to appear twice here, which is only
        // visible as wasted work — the second pass overwrote the same values.
        let mut context = stats.clone();
        if let Some(variables) = rule
            .body
            .get("variables")
            .and_then(serde_json::Value::as_object)
            && let Some(map) = context.as_object_mut()
        {
            for (name, value) in variables {
                map.insert(name.clone(), value.clone());
            }
        }
        match evaluate_with_actions(&rule.body, &context) {
            Ok((false, _)) => {}
            Ok((true, actions)) => {
                match budget(&queries, &rule.key, &rule.body, player_id).await {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(error) => {
                        tracing::warn!(
                            "[rules] '{}' limits could not be checked: {error:?}",
                            rule.key
                        );
                        continue;
                    }
                }
                matched += 1;
                apply(plane, player_id, &rule.key, &actions).await;
                // Recorded after the actions, so a rule whose apply is aborted
                // mid-way has not spent the player's one allowance on a reward
                // that never arrived.
                if let Err(error) = queries.record_rule_fire(&rule.key, player_id).await {
                    tracing::warn!("[rules] '{}' fire not recorded: {error}", rule.key);
                }
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

/// Whether the rule may pay this player again.
///
/// A rule with no `limits` is unlimited, which keeps every rule written before
/// limits existed firing exactly as it did.
async fn budget(
    queries: &std::sync::Arc<crate::repositories::db::Queries>,
    rule_key: &str,
    body: &serde_json::Value,
    player_id: Uuid,
) -> Result<bool, crate::error::V2Error> {
    // A rule whose limits do not parse cannot be trusted to be bounded, so it
    // does not fire. This is the one case where skipping is safer than running.
    let limits = parse_limits(body, "")
        .map_err(|error| V2Error::invalid_argument(format!("Invalid rule limits at {error}")))?;
    if limits.is_unlimited() {
        return Ok(true);
    }
    let Some(previous) = queries.get_rule_fire(rule_key, player_id).await? else {
        return Ok(true);
    };
    if limits.max_per_player > 0 && previous.count >= limits.max_per_player {
        return Ok(false);
    }
    if limits.cooldown_secs > 0 {
        if let Some(last) = previous.last_fire {
            let elapsed = (chrono::Utc::now() - last).num_seconds().max(0) as u64;
            if elapsed < limits.cooldown_secs {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// Apply one matched rule's actions.
///
/// Each action is applied on its own. A rule that grants a currency and an item
/// whose definition has since been deleted still delivers the currency, because
/// the alternative is a reward that silently does nothing at all.
async fn apply(plane: &Plane, player_id: Uuid, rule: &str, actions: &[Action]) {
    for action in actions {
        if let Err(error) = apply_one(plane, player_id, action).await {
            tracing::warn!(
                "[rules] '{}' action {} failed for {player_id}: {error:?}",
                rule,
                action.kind()
            );
        }
    }
}

async fn apply_one(plane: &Plane, player_id: Uuid, action: &Action) -> Result<(), V2Error> {
    let queries = plane.q().await?;
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
            let inventory =
                crate::services::inventory::InventoryService::new(Arc::new(plane.clone()));
            // grant_item answers with the updated stock Json; a rule grant
            // has no reply channel, so the value is intentionally discarded
            // (`?` above still propagates a failed grant).
            let _ = inventory
                .grant_item(
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
