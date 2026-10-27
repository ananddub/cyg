//! The tenant this process hosts.
//!
//! One process runs one game, but the database and the console are shared. Live
//! telemetry has no tenant column of its own, so the process has to know which
//! registered game it belongs to before it can answer a scoped dashboard request
//! or stamp the players it creates. Identity comes from `config.toml`
//! (`game_id`/`game_key`); a database that holds exactly one active game is the
//! fallback, so a single-tenant deployment needs no extra configuration.

use std::sync::Arc;

use uuid::Uuid;

use crate::ApiState;
use crate::dto::console::games::GameRow;
use crate::error::V2Error;
use crate::repositories::console::ListStorageRow;
use crate::repositories::db::Queries;
use crate::repositories::models::{Groups, Leaderboard, Purchase, Subscription, Users};

/// The FlashORM wrapper every tenancy read and write goes through; 503 when
/// the database is not configured (the same refusal the raw pool path gave).
fn q(state: &ApiState) -> Result<Arc<Queries>, V2Error> {
    state.q.clone().ok_or_else(V2Error::no_db)
}

/// The generated tenancy rows alias the console projections
/// (`TenancyStoragePageRow` IS `ListStorageRow`, the game rows alias
/// `ListGamesRow` whose `From<GameRow>` bridge lives in games/service), so the
/// page fns keep their signatures unchanged.

const SELECT_BY_ID: &str = "SELECT id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at FROM games WHERE id = $1;";

const SELECT_BY_KEY: &str = "SELECT id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at FROM games WHERE game_key = $1;";

const SELECT_SOLE_ACTIVE: &str = "SELECT id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at FROM games WHERE status = 'active' ORDER BY created_at ASC LIMIT 2;";

/// The game this process hosts, if one can be resolved.
pub async fn resolve(state: &ApiState) -> Result<Option<GameRow>, V2Error> {
    if let Some(raw) = configured(state.game.game_id.as_deref()) {
        let id = Uuid::parse_str(raw)
            .map_err(|_| V2Error::internal(format!("Configured game_id '{raw}' is not a UUID.")))?;
        let game = by_id(state, id).await?;
        if game.is_none() {
            tracing::warn!("[tenancy] configured game_id {id} is not registered");
        }
        return Ok(game);
    }
    if let Some(key) = configured(state.game.game_key.as_deref()) {
        let game = by_key(state, key).await?;
        if game.is_none() {
            tracing::warn!("[tenancy] configured game_key '{key}' is not registered");
        }
        return Ok(game);
    }
    if let Some(name) = configured(Some(&state.game.name)) {
        if let Ok(Some(game)) = by_name(state, name).await {
            return Ok(Some(game));
        }
        // Auto-register game in database if not yet registered
        match auto_register(state, name).await {
            Ok(game) => {
                tracing::info!(
                    "[tenancy] auto-registered game '{}' ({})",
                    game.name,
                    game.game_key
                );
                return Ok(Some(game));
            }
            Err(err) => {
                tracing::debug!("[tenancy] auto-registration skipped: {}", err.message);
            }
        }
    }
    sole_active(state).await
}

/// The hosted game's id, or `None` when identity cannot be resolved.
pub async fn game_id(state: &ApiState) -> Result<Option<Uuid>, V2Error> {
    Ok(resolve(state).await?.map(|game| game.id))
}

/// Refuse a dashboard request that names a game this process does not host.
///
/// The telemetry feed is process-wide, so serving it under another game's
/// workspace would present this game's numbers as that one's.
pub async fn require_hosted(state: &ApiState, claimed: Uuid) -> Result<(), V2Error> {
    // In multi-game OpenOxide, any registered game in the database is hosted and valid!
    let _ = crate::services::games::GamesService::find_game(state, claimed).await?;
    Ok(())
}

/// Claim rows that predate tenancy for the hosted game.
///
/// Existing player accounts, boards and other player rows were written before
/// `game_id` existed. The first process to resolve its tenant adopts them; later
/// rows carry the tenant from creation. Both statements are idempotent and a
/// missing table is a no-op, so a new database and an old one take the same path.
pub async fn backfill(state: &ApiState, game_id: Uuid) {
    let Ok(queries) = q(state) else {
        return;
    };
    for (table, claimed) in [
        ("users", queries.backfill_users_game(game_id).await),
        ("leaderboard", queries.backfill_leaderboard_game(game_id).await),
    ] {
        match claimed {
            Ok(done) => {
                if done.rows_affected() > 0 {
                    tracing::info!(
                        "[tenancy] claimed {} legacy {table} row(s) for game {game_id}",
                        done.rows_affected()
                    );
                }
            }
            Err(err) => {
                tracing::info!("[tenancy] {table} backfill skipped: {err}");
            }
        }
    }
}

/// Stamp a player account created by this process with its tenant. A failed
/// update is logged and ignored: the account still exists, and the next process
/// boot's backfill claims it.
pub async fn stamp_user(state: &ApiState, user_id: Uuid) {
    let Ok(queries) = q(state) else {
        return;
    };
    let Ok(Some(game_id)) = game_id(state).await else {
        return;
    };
    if let Err(err) = queries.stamp_user_game(game_id, user_id).await {
        tracing::warn!("[tenancy] could not stamp user {user_id}: {err}");
    }
}

/// Stamp a player account with a specific tenant. Used by the console import,
/// where the row is restored into the game whose workspace imported it rather
/// than the game this process happens to host.
pub async fn stamp_user_as(state: &ApiState, user_id: Uuid, game_id: Option<Uuid>) {
    let (Ok(queries), Some(game_id)) = (q(state), game_id) else {
        return;
    };
    if let Err(err) = queries.stamp_user_game(game_id, user_id).await {
        tracing::warn!("[tenancy] could not stamp user {user_id}: {err}");
    }
}

// ------------------------------------------------------------------ reads









pub async fn list_accounts(
    state: &ApiState,
    game: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Users>, V2Error> {
    // Hand-written with a nullable uuid: a NULL game id means the UNscoped
    // (landing) listing, not an empty one, and the generator has no nullable
    // parameters to say that with.
    sqlx::query_as::<_, Users>(ACCOUNTS_PAGE)
        .bind(game)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool(state)?)
        .await
        .map_err(V2Error::from_sqlx)
}

pub async fn list_groups(
    state: &ApiState,
    game: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Groups>, V2Error> {
    // Nullable game id — see list_accounts.
    sqlx::query_as::<_, Groups>(GROUP_PAGE)
        .bind(game)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool(state)?)
        .await
        .map_err(V2Error::from_sqlx)
}

pub async fn list_boards(
    state: &ApiState,
    game: Option<Uuid>,
    category: i16,
    limit: i64,
    offset: i64,
) -> Result<Vec<Leaderboard>, V2Error> {
    // Nullable game id — see list_accounts.
    sqlx::query_as::<_, Leaderboard>(BOARDS_PAGE)
        .bind(category)
        .bind(game)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool(state)?)
        .await
        .map_err(V2Error::from_sqlx)
}

pub async fn list_storage(
    state: &ApiState,
    game: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ListStorageRow>, V2Error> {
    // Nullable game id — see list_accounts.
    sqlx::query_as::<_, ListStorageRow>(STORAGE_PAGE)
        .bind(game)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool(state)?)
        .await
        .map_err(V2Error::from_sqlx)
}

pub async fn storage_collections(state: &ApiState, game: Option<Uuid>) -> Result<Vec<String>, V2Error> {
    // Nullable game id — see list_accounts.
    sqlx::query_scalar::<_, String>(STORAGE_COLLECTIONS)
        .bind(game)
        .fetch_all(pool(state)?)
        .await
        .map_err(V2Error::from_sqlx)
}

pub async fn storage_in_collection(
    state: &ApiState,
    game: Option<Uuid>,
    collection: &str,
    limit: i64,
) -> Result<Vec<ListStorageRow>, V2Error> {
    // Nullable game id — see list_accounts.
    sqlx::query_as::<_, ListStorageRow>(STORAGE_COLLECTION)
        .bind(collection)
        .bind(game)
        .bind(limit)
        .fetch_all(pool(state)?)
        .await
        .map_err(V2Error::from_sqlx)
}

pub async fn list_purchases(
    state: &ApiState,
    game: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Purchase>, V2Error> {
    // Nullable game id — see list_accounts.
    sqlx::query_as::<_, Purchase>(PURCHASES_PAGE)
        .bind(game)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool(state)?)
        .await
        .map_err(V2Error::from_sqlx)
}

pub async fn list_subscriptions(
    state: &ApiState,
    game: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Subscription>, V2Error> {
    // Nullable game id — see list_accounts.
    sqlx::query_as::<_, Subscription>(SUBSCRIPTIONS_PAGE)
        .bind(game)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool(state)?)
        .await
        .map_err(V2Error::from_sqlx)
}

// ------------------------------------------------------------------ reads
//
// The page reads stay hand-written on purpose: their nullable game id carries
// the "unscoped listing" case (NULL = no game filter), which the generator's
// non-null parameters cannot express. Everything with a fixed shape — tenant
// resolution, backfill, stamps — is generated.









// ------------------------------------------------------------------ reads
//
// The page reads stay hand-written on purpose: their nullable game id carries
// the "unscoped listing" case (NULL = no game filter), which the generator's
// non-null parameters cannot express. Everything with a fixed shape — tenant
// resolution, backfill, stamps — is generated.

const ACCOUNTS_PAGE: &str = "SELECT * FROM users WHERE ($1::uuid IS NULL OR game_id = $1::uuid) ORDER BY create_time DESC, id ASC LIMIT $2 OFFSET $3";

const GROUP_PAGE: &str = "SELECT * FROM groups WHERE disable_time = '1970-01-01 00:00:00+00' AND ($1::uuid IS NULL OR creator_id IN (SELECT id FROM users WHERE game_id = $1::uuid)) ORDER BY update_time ASC, edge_count ASC, id ASC LIMIT $2 OFFSET $3";

const BOARDS_PAGE: &str = "SELECT * FROM leaderboard WHERE category = $1 AND ($2::uuid IS NULL OR game_id = $2::uuid) ORDER BY create_time DESC, id ASC LIMIT $3 OFFSET $4";

const STORAGE_PAGE: &str = "SELECT collection, key AS object_key, user_id, value::text AS value, version, read, write, create_time, update_time FROM storage s WHERE ($1::uuid IS NULL OR s.user_id IN (SELECT id FROM users WHERE game_id = $1::uuid)) ORDER BY update_time DESC, collection ASC, key ASC LIMIT $2 OFFSET $3";

const STORAGE_COLLECTION: &str = "SELECT collection, key AS object_key, user_id, value::text AS value, version, read, write, create_time, update_time FROM storage s WHERE collection = $1 AND ($2::uuid IS NULL OR s.user_id IN (SELECT id FROM users WHERE game_id = $2::uuid)) ORDER BY key ASC, user_id ASC LIMIT $3";

const STORAGE_COLLECTIONS: &str = "SELECT DISTINCT s.collection FROM storage s WHERE ($1::uuid IS NULL OR s.user_id IN (SELECT id FROM users WHERE game_id = $1::uuid)) ORDER BY s.collection ASC";

const PURCHASES_PAGE: &str = "SELECT p.* FROM purchase p WHERE ($1::uuid IS NULL OR p.user_id IN (SELECT id FROM users WHERE game_id = $1::uuid)) ORDER BY p.purchase_time DESC, p.transaction_id ASC LIMIT $2 OFFSET $3";

const SUBSCRIPTIONS_PAGE: &str = "SELECT s.* FROM subscription s WHERE ($1::uuid IS NULL OR s.user_id IN (SELECT id FROM users WHERE game_id = $1::uuid)) ORDER BY s.purchase_time DESC, s.original_transaction_id ASC LIMIT $2 OFFSET $3";

// ----------------------------------------------------------------- guards

/// Whether an account belongs to the scoped game. An unscoped (landing) call is
/// not asked to prove anything, so it stays addressable exactly as before.
pub async fn account_in_game(
    state: &ApiState,
    game: Option<Uuid>,
    user_id: Uuid,
) -> Result<bool, V2Error> {
    let Some(game) = game else {
        return Ok(true);
    };
    // Hand-written: the generator mis-types `users.id` inside a correlated
    // EXISTS (it inferred i32 for a uuid column), so this one guard stays raw.
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1 AND game_id = $2)",
    )
    .bind(user_id)
    .bind(game)
    .fetch_one(pool(state)?)
    .await
    .map_err(V2Error::from_sqlx)?;
    Ok(exists)
}

pub async fn require_account(
    state: &ApiState,
    game: Option<Uuid>,
    user_id: Uuid,
) -> Result<(), V2Error> {
    if account_in_game(state, game, user_id).await? {
        Ok(())
    } else {
        Err(V2Error::not_found("Account not found."))
    }
}

/// A group belongs to the game its creator plays in. An unscoped call leaves the
/// existing addressable set untouched.
pub async fn require_group(
    state: &ApiState,
    game: Option<Uuid>,
    group_id: Uuid,
) -> Result<(), V2Error> {
    let Some(game) = game else {
        return Ok(());
    };
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM groups g WHERE g.id = $1 AND g.creator_id IN (SELECT id FROM users WHERE game_id = $2))",
    )
    .bind(group_id)
    .bind(game)
    .fetch_one(pool(state)?)
    .await
    .map_err(V2Error::from_sqlx)?;
    if exists {
        Ok(())
    } else {
        Err(V2Error::not_found("Group not found."))
    }
}

/// A board (or tournament) belongs to the game that authored it. An unscoped call
/// keeps every board addressable.
pub async fn board_in_game(
    state: &ApiState,
    game: Option<Uuid>,
    board_id: &str,
) -> Result<bool, V2Error> {
    let Some(game) = game else {
        return Ok(true);
    };
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM leaderboard WHERE id = $1 AND game_id = $2)",
    )
    .bind(board_id)
    .bind(game)
    .fetch_one(pool(state)?)
    .await
    .map_err(V2Error::from_sqlx)?;
    Ok(exists)
}

// ----------------------------------------------------------------- writes

pub async fn delete_storage_collection(
    state: &ApiState,
    game: Option<Uuid>,
    collection: &str,
) -> Result<u64, V2Error> {
    let done = sqlx::query(
        "DELETE FROM storage s WHERE s.collection = $1 AND ($2::uuid IS NULL OR s.user_id IN (SELECT id FROM users WHERE game_id = $2::uuid))",
    )
    .bind(collection)
    .bind(game)
    .execute(pool(state)?)
    .await
    .map_err(V2Error::from_sqlx)?;
    Ok(done.rows_affected())
}

pub async fn delete_notification(
    state: &ApiState,
    game: Option<Uuid>,
    id: Uuid,
) -> Result<bool, V2Error> {
    let done = sqlx::query(
        "DELETE FROM notification n WHERE n.id = $1 AND ($2::uuid IS NULL OR n.user_id IN (SELECT id FROM users WHERE game_id = $2::uuid))",
    )
    .bind(id)
    .bind(game)
    .execute(pool(state)?)
    .await
    .map_err(V2Error::from_sqlx)?;
    Ok(done.rows_affected() > 0)
}

pub async fn delete_subscriptions_for_product(
    state: &ApiState,
    game: Option<Uuid>,
    product_id: &str,
) -> Result<u64, V2Error> {
    let done = sqlx::query(
        "DELETE FROM subscription s WHERE s.product_id = $1 AND ($2::uuid IS NULL OR s.user_id IN (SELECT id FROM users WHERE game_id = $2::uuid))",
    )
    .bind(product_id)
    .bind(game)
    .execute(pool(state)?)
    .await
    .map_err(V2Error::from_sqlx)?;
    Ok(done.rows_affected())
}

fn pool(state: &ApiState) -> Result<&sqlx::PgPool, V2Error> {
    state.db.as_ref().ok_or_else(V2Error::no_db)
}

fn configured(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

async fn by_id(state: &ApiState, id: Uuid) -> Result<Option<GameRow>, V2Error> {
    Ok(q(state)?
        .tenancy_game_by_id(id)
        .await
        .map_err(V2Error::from_sqlx)?
        .into_iter()
        .next()
        .map(GameRow::from))
}

async fn by_key(state: &ApiState, key: &str) -> Result<Option<GameRow>, V2Error> {
    Ok(q(state)?
        .tenancy_game_by_key(key)
        .await
        .map_err(V2Error::from_sqlx)?
        .into_iter()
        .next()
        .map(GameRow::from))
}

async fn by_name(state: &ApiState, name: &str) -> Result<Option<GameRow>, V2Error> {
    let pool = pool(state)?;
    let row = sqlx::query_as::<_, GameRow>(
        "SELECT id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at \
         FROM games WHERE (LOWER(name) = LOWER($1) OR LOWER(game_key) = LOWER($1)) AND status = 'active' \
         ORDER BY created_at DESC LIMIT 1;"
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(V2Error::from_sqlx)?;
    Ok(row)
}

async fn auto_register(state: &ApiState, name: &str) -> Result<GameRow, V2Error> {
    let dto = crate::services::games::GamesService::create_game(
        state,
        &crate::dto::console::games::CreateGameRequest {
            name: name.to_string(),
            engine_type: Some("custom".to_string()),
            tick_rate: Some(state.game.tick_rate),
            max_clients: Some(state.game.max_clients.max(1) as u32),
            persistent_rooms: None,
            auto_close_seconds: None,
            config: None,
        },
    )
    .await?;
    by_id(state, dto.id)
        .await?
        .ok_or_else(|| V2Error::internal("Auto-registered game could not be re-read"))
}

/// A database with exactly one active game is unambiguous. Two means the
/// operator has to name the tenant rather than have one guessed.
async fn sole_active(state: &ApiState) -> Result<Option<GameRow>, V2Error> {
    let mut rows = q(state)?
        .tenancy_sole_active_games()
        .await
        .map_err(V2Error::from_sqlx)?;
    if rows.len() == 1 {
        Ok(rows.pop().map(GameRow::from))
    } else {
        Ok(None)
    }
}
