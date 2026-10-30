use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::error::V2Error;

/// The validated shape of a quest or daily reward.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Reward {
    pub currency: Vec<(String, f64)>,
    pub items: Vec<(String, i32)>,
}

impl Reward {
    pub(crate) fn is_empty(&self) -> bool {
        self.currency.is_empty() && self.items.is_empty()
    }

    pub(crate) fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "currency": self
                .currency
                .iter()
                .map(|(currency, amount)| (currency.clone(), serde_json::json!(amount)))
                .collect::<serde_json::Map<_, _>>(),
            "items": self
                .items
                .iter()
                .map(|(item_id, quantity)| {
                    serde_json::json!({ "item_id": item_id, "quantity": quantity })
                })
                .collect::<Vec<_>>(),
        })
    }
}

/// Reads `{"currency": {"gold": 100}, "items": [{"item_id": "sword",
/// "quantity": 2}]}`. Bad shape is refused while the operator is typing it, not
/// later while a player is trying to claim it.
pub(crate) fn parse_reward(value: &serde_json::Value) -> Result<Reward, V2Error> {
    let Some(object) = value.as_object() else {
        return match value {
            serde_json::Value::Null => Ok(Reward::default()),
            _ => Err(V2Error::invalid_argument(
                "A reward must be an object of `currency` and `items`.",
            )),
        };
    };
    let unknown = object
        .keys()
        .map(String::as_str)
        .filter(|key| *key != "currency" && *key != "items")
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(V2Error::invalid_argument(format!(
            "A reward only knows `currency` and `items`, not {}.",
            unknown.join(", ")
        )));
    }
    let mut reward = Reward::default();
    if let Some(currency) = object.get("currency") {
        let currency = currency
            .as_object()
            .ok_or_else(|| V2Error::invalid_argument("A reward's `currency` must be an object."))?;
        for (name, amount) in currency {
            let amount = amount.as_f64().ok_or_else(|| {
                V2Error::invalid_argument(format!("The {name} reward amount must be a number."))
            })?;
            if amount < 0.0 {
                return Err(V2Error::invalid_argument(
                    "A reward cannot take currency away.",
                ));
            }
            reward.currency.push((name.clone(), amount));
        }
    }
    if let Some(items) = object.get("items") {
        let items = items
            .as_array()
            .ok_or_else(|| V2Error::invalid_argument("A reward's `items` must be a list."))?;
        for item in items {
            let entry = item
                .as_object()
                .ok_or_else(|| V2Error::invalid_argument("A reward item must be an object."))?;
            let item_id = entry
                .get("item_id")
                .and_then(serde_json::Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .ok_or_else(|| V2Error::invalid_argument("A reward item needs an `item_id`."))?;
            let quantity = match entry.get("quantity") {
                None => 1,
                Some(quantity) => quantity
                    .as_i64()
                    .filter(|n| *n > 0 && *n <= 65_535)
                    .ok_or_else(|| {
                        V2Error::invalid_argument(
                            "A reward item's `quantity` must be a positive whole number.",
                        )
                    })? as i32,
            };
            reward.items.push((item_id.to_owned(), quantity));
        }
    }
    Ok(reward)
}

/// A stable row id for one payout, so the same reward written twice lands on the
/// same ledger and inventory rows instead of creating a second copy.
pub(crate) fn payout_uuid(seed: &str) -> Uuid {
    let digest = Sha256::digest(seed.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Uuid::from_bytes(bytes)
}
