use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::ApiState;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::models::{PlayerQuest, QuestDefinition, Season};

/// Definitions belong to the hosted game. A row that carries no game — a single
/// game install, or a definition written before tenancy — stays visible.
pub(crate) async fn owned_definitions(
    queries: &Arc<Queries>,
    state: &ApiState,
) -> Result<Vec<QuestDefinition>, V2Error> {
    let game_id = crate::services::games::tenant::game_id(state).await?;
    Ok(queries
        .list_quest_definitions()
        .await?
        .into_iter()
        .filter(|definition| {
            definition.game_id.is_none() || game_id.is_none() || definition.game_id == game_id
        })
        .collect())
}

pub(crate) async fn definition_of(
    queries: &Arc<Queries>,
    state: &ApiState,
    quest_key: &str,
) -> Result<QuestDefinition, V2Error> {
    let missing = || V2Error::not_found(format!("Quest '{quest_key}' does not exist."));
    let definition = queries
        .get_quest_definition(quest_key)
        .await?
        .ok_or_else(missing)?;
    let game_id = crate::services::games::tenant::game_id(state).await?;
    let owned = definition.game_id.is_none() || game_id.is_none() || definition.game_id == game_id;
    owned.then_some(definition).ok_or_else(missing)
}

/// The definitions a player can act on right now.
pub(crate) fn visible<'a>(
    definitions: &'a [QuestDefinition],
    seasons: &BTreeMap<String, Season>,
    now: DateTime<Utc>,
) -> Vec<&'a QuestDefinition> {
    definitions
        .iter()
        .filter(|definition| runs_now(definition, seasons, now))
        .collect()
}

/// Of the visible definitions, the ones this player's live subscriptions pay
/// for: a quest that names a product nobody is subscribed to is not offered.
pub(crate) fn entitled<'a>(
    definitions: &'a [QuestDefinition],
    seasons: &BTreeMap<String, Season>,
    now: DateTime<Utc>,
    products: &BTreeSet<String>,
) -> Vec<&'a QuestDefinition> {
    visible(definitions, seasons, now)
        .into_iter()
        .filter(|definition| match definition.requires_product.as_deref() {
            None => true,
            Some(product) => products.contains(product),
        })
        .collect()
}

/// Seasons that have started and not ended, keyed by their id.
pub(crate) async fn open_seasons(
    queries: &Arc<Queries>,
    now: DateTime<Utc>,
) -> Result<BTreeMap<String, Season>, V2Error> {
    Ok(queries
        .list_seasons()
        .await?
        .into_iter()
        .filter(|season| season.starts_at <= now && now < season.ends_at)
        .map(|season| (season.id.clone(), season))
        .collect())
}

/// A quest runs when its own window covers `now` and, when it names a season,
/// that season is open.
pub(crate) fn runs_now(
    definition: &QuestDefinition,
    seasons: &BTreeMap<String, Season>,
    now: DateTime<Utc>,
) -> bool {
    definition.active_from <= now
        && now < definition.active_to
        && match definition.season.as_deref() {
            None => true,
            Some(season) => seasons.contains_key(season),
        }
}

pub(crate) async fn progress_by_quest(
    queries: &Arc<Queries>,
    user_id: Uuid,
) -> Result<BTreeMap<String, PlayerQuest>, V2Error> {
    Ok(queries
        .list_player_quests(user_id)
        .await?
        .into_iter()
        .map(|row| (row.quest_key.clone(), row))
        .collect())
}

/// Every counter the player has, by name. A quest's progress is read from the
/// counter it counts, so opening a season mid-run does not lose the plays that
/// happened while it was closed.
pub(crate) async fn stats_by_player(
    queries: &Arc<Queries>,
    user_id: Uuid,
) -> Result<BTreeMap<String, i64>, V2Error> {
    Ok(queries
        .list_player_stats(user_id)
        .await?
        .into_iter()
        .map(|row| (row.stat, row.value))
        .collect())
}

/// Keys an operator types into a URL: quest and season keys.
pub(crate) fn definition_key(raw: &str, what: &str) -> Result<String, V2Error> {
    let key = raw.trim().to_lowercase();
    let acceptable = key.len() <= 64
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'));
    if key.is_empty() {
        return Err(V2Error::invalid_argument(format!("A {what} needs a key.")));
    }
    if !acceptable {
        return Err(V2Error::invalid_argument(format!(
            "A {what} key is 1-64 characters of lowercase letters, digits, dash or underscore."
        )));
    }
    Ok(key)
}

/// Counter names are lowercase identifiers: the same string is the event name an
/// integrator sends and the `goal_kind` a definition points at, so both have to
/// agree on spelling.
pub(crate) fn counter_name(raw: &str) -> Result<&str, V2Error> {
    let name = raw.trim();
    let acceptable = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if acceptable {
        Ok(name)
    } else {
        Err(V2Error::invalid_argument(
            "A counter name is 1-64 characters of lowercase letters, digits or underscore.",
        ))
    }
}
