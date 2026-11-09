//! What a rule does once it matches.
//!
//! The expression tree answers yes or no; an action is what the operator asked
//! for when the answer is yes. Keeping them in the same document means a rule is
//! one thing to publish, version and roll back, rather than a condition that
//! points at a separately-versioned reward.
//!
//! Actions are validated at publish time, not at fire time. An operator who
//! mistypes an item id should learn it from the console, while they are looking
//! at the rule, not from a player asking why their reward never arrived.

use serde_json::Value;

use super::eval::RuleError;

/// One thing a matching rule does.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Move a virtual currency. Negative amounts are allowed so a rule can
    /// charge as well as reward, which is what a shop discount needs.
    Currency { currency: String, amount: f64 },
    /// Add items. The item definition must exist by the time the rule fires,
    /// but it is not checked here: a rule may be published before the catalogue
    /// entry it grants is imported.
    Item { item_id: String, quantity: i64 },
    /// Send an inbox message to the player.
    Notify { subject: String, body: String },
    /// Attach a badge.
    Badge { badge: String },
}

/// The highest number of actions one rule may carry.
///
/// A rule with more is a sign the operator wanted several rules, and the cap
/// keeps one matching rule from turning into an unbounded burst of writes.
pub(super) const MAX_ACTIONS: usize = 16;

impl Action {
    pub fn kind(&self) -> &'static str {
        match self {
            Action::Currency { .. } => "currency",
            Action::Item { .. } => "item",
            Action::Notify { .. } => "notify",
            Action::Badge { .. } => "badge",
        }
    }
}

/// Read the `actions` array of a rule body.
///
/// A missing or null `actions` is not an error: every rule written before
/// actions existed is a condition-only rule, and those must keep working.
pub(crate) fn parse_actions(body: &Value, path: &str) -> Result<Vec<Action>, RuleError> {
    let Some(raw) = body.get("actions").filter(|value| !value.is_null()) else {
        return Ok(Vec::new());
    };
    let items = raw.as_array().ok_or_else(|| RuleError {
        path: format!("{path}.actions"),
        message: "actions is an array".to_owned(),
    })?;
    if items.len() > MAX_ACTIONS {
        return Err(RuleError {
            path: format!("{path}.actions"),
            message: format!("a rule carries at most {MAX_ACTIONS} actions"),
        });
    }
    items
        .iter()
        .enumerate()
        .map(|(index, item)| parse_one(item, &format!("{path}.actions[{index}]")))
        .collect()
}

fn parse_one(body: &Value, path: &str) -> Result<Action, RuleError> {
    let at = |field: &str, message: &str| RuleError {
        path: format!("{path}.{field}"),
        message: message.to_owned(),
    };
    let object = body
        .as_object()
        .ok_or_else(|| at("", "an action is an object with a type"))?;

    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| at("type", "type is one of currency, item, notify, badge"))?;

    let text = |field: &str| -> Result<String, RuleError> {
        object
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| at(field, &format!("{kind} needs a non-empty {field}")))
    };

    match kind {
        "currency" => {
            let currency = text("currency")?;
            let amount = object
                .get("amount")
                .and_then(Value::as_f64)
                .ok_or_else(|| at("amount", "currency needs a numeric amount"))?;
            if amount == 0.0 {
                return Err(at("amount", "a zero amount does nothing — leave it out"));
            }
            Ok(Action::Currency { currency, amount })
        }
        "item" => {
            let item_id = text("item_id")?;
            let quantity = match object.get("quantity") {
                None | Some(Value::Null) => 1,
                Some(value) => value
                    .as_i64()
                    .filter(|quantity| *quantity > 0)
                    .ok_or_else(|| at("quantity", "quantity is a positive whole number"))?,
            };
            Ok(Action::Item { item_id, quantity })
        }
        "notify" => Ok(Action::Notify {
            subject: text("subject")?,
            body: object
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
        }),
        "badge" => Ok(Action::Badge {
            badge: text("badge")?,
        }),
        other => Err(at(
            "type",
            &format!("{other} is not a type — use currency, item, notify or badge"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_rule_without_actions_has_none() {
        assert!(parse_actions(&json!({"all": []}), "").unwrap().is_empty());
        assert!(
            parse_actions(&json!({"all": [], "actions": null}), "")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn each_kind_reads_its_fields() {
        let body = json!({
            "value": true,
            "actions": [
                {"type": "currency", "currency": "gems", "amount": 100},
                {"type": "item", "item_id": "sword"},
                {"type": "notify", "subject": "Hi"},
                {"type": "badge", "badge": "founder"}
            ]
        });
        let actions = parse_actions(&body, "").unwrap();
        assert_eq!(actions.len(), 4);
        assert_eq!(
            actions[0],
            Action::Currency {
                currency: "gems".into(),
                amount: 100.0
            }
        );
        assert_eq!(
            actions[1],
            Action::Item {
                item_id: "sword".into(),
                quantity: 1
            }
        );
    }

    #[test]
    fn a_zero_amount_is_refused() {
        let body = json!({"value": true, "actions": [{"type": "currency", "currency": "gems", "amount": 0}]});
        assert!(parse_actions(&body, "").is_err());
    }

    #[test]
    fn an_unknown_type_names_the_alternatives() {
        let body = json!({"value": true, "actions": [{"type": "teleport"}]});
        let error = parse_actions(&body, "").unwrap_err();
        assert!(error.message.contains("currency, item, notify or badge"));
    }

    #[test]
    fn too_many_actions_are_refused() {
        let actions: Vec<Value> = (0..MAX_ACTIONS + 1)
            .map(|_| json!({"type": "badge", "badge": "b"}))
            .collect();
        let body = json!({"value": true, "actions": actions});
        assert!(parse_actions(&body, "").is_err());
    }
}
