//! The console statements FlashORM cannot generate, mirrored in
//! `db/manual/console.sql`.
//!
//! Two are out of reach: the account import, which is one transaction over six
//! statements and so cannot be a single generated method, and the storage deletes,
//! whose `key` column the query parser reserves. They take the pool rather than the
//! [`Queries`](crate::repositories::db::Queries) wrapper, exactly as the storage
//! service's hand-written statements do.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::repositories::console::ListStorageRow;
use crate::repositories::console_mutations::ListUserEdgesRow;
use crate::repositories::models::Users;

/// One sign-in provider row of an import.
pub struct ImportDeviceParams {
    pub id: String,
    pub provider: String,
}

/// One note of an import. It carries no author: the author was a console operator,
/// and that operator may not exist on the deployment being imported into.
pub struct ImportNoteParams {
    pub id: Uuid,
    pub note: String,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

/// One wallet ledger entry of an import.
pub struct ImportLedgerParams {
    pub id: Uuid,
    pub changeset: serde_json::Value,
    pub metadata: serde_json::Value,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

/// An account and everything hanging off it, handed over as one unit so a failure
/// halfway through leaves nothing behind.
pub struct ImportAccountParams {
    pub account: Users,
    pub devices: Vec<ImportDeviceParams>,
    pub notes: Vec<ImportNoteParams>,
    pub wallet_ledger: Vec<ImportLedgerParams>,
    pub storage: Vec<ListStorageRow>,
    pub edges: Vec<ListUserEdgesRow>,
}

/// Restore an account whole, in one transaction.
///
/// The id must be free, so an import can never merge two accounts into one, and
/// every row keeps the timestamp it was exported with: a restore that stamped
/// everything with "now" would rewrite the history the export was taken to
/// preserve. Edges are the one thing that may already be there — the other end's
/// own edge can name this account — so those are `DO NOTHING` rather than fatal.
pub async fn import_account_bundle(
    pool: &sqlx::PgPool,
    params: &ImportAccountParams,
) -> Result<(), sqlx::Error> {
    let account = &params.account;
    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO users (id, username, display_name, avatar_url, lang_tag, location, timezone, metadata, wallet, email, facebook_id, google_id, gamecenter_id, steam_id, custom_id, edge_count, create_time, update_time, verify_time, disable_time, facebook_instant_game_id, apple_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22);",
    )
    .bind(account.id)
    .bind(&account.username)
    .bind(&account.display_name)
    .bind(&account.avatar_url)
    .bind(&account.lang_tag)
    .bind(&account.location)
    .bind(&account.timezone)
    .bind(&account.metadata)
    .bind(&account.wallet)
    .bind(&account.email)
    .bind(&account.facebook_id)
    .bind(&account.google_id)
    .bind(&account.gamecenter_id)
    .bind(&account.steam_id)
    .bind(&account.custom_id)
    .bind(account.edge_count)
    .bind(account.create_time)
    .bind(account.update_time)
    .bind(account.verify_time)
    .bind(account.disable_time)
    .bind(&account.facebook_instant_game_id)
    .bind(&account.apple_id)
    .execute(&mut *tx)
    .await?;

    for device in &params.devices {
        sqlx::query("INSERT INTO user_device (id, user_id, provider) VALUES ($1, $2, $3);")
            .bind(&device.id)
            .bind(account.id)
            .bind(&device.provider)
            .execute(&mut *tx)
            .await?;
    }

    for note in &params.notes {
        sqlx::query(
            "INSERT INTO users_notes (id, user_id, note, create_time, update_time) VALUES ($1, $2, $3, $4, $5);",
        )
        .bind(note.id)
        .bind(account.id)
        .bind(&note.note)
        .bind(note.create_time)
        .bind(note.update_time)
        .execute(&mut *tx)
        .await?;
    }

    for entry in &params.wallet_ledger {
        sqlx::query(
            "INSERT INTO wallet_ledger (id, user_id, changeset, metadata, create_time, update_time) VALUES ($1, $2, $3, $4, $5, $6);",
        )
        .bind(entry.id)
        .bind(account.id)
        .bind(&entry.changeset)
        .bind(&entry.metadata)
        .bind(entry.create_time)
        .bind(entry.update_time)
        .execute(&mut *tx)
        .await?;
    }

    for object in &params.storage {
        sqlx::query(
            "INSERT INTO storage (collection, key, user_id, value, version, read, write, create_time, update_time) VALUES ($1, $2, $3, $4::jsonb, $5, $6, $7, $8, $9);",
        )
        .bind(&object.collection)
        .bind(&object.object_key)
        .bind(account.id)
        // The export carries the stored JSON verbatim, so it is cast back rather
        // than re-encoded.
        .bind(&object.value)
        .bind(&object.version)
        .bind(object.read)
        .bind(object.write)
        .bind(object.create_time)
        .bind(object.update_time)
        .execute(&mut *tx)
        .await?;
    }

    for edge in &params.edges {
        sqlx::query(
            "INSERT INTO user_edge (source_id, destination_id, state, position, metadata) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (source_id, state, position) DO NOTHING;",
        )
        .bind(account.id)
        .bind(edge.destination_id)
        .bind(edge.state)
        .bind(edge.position)
        .bind(&edge.metadata)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// One owner's object in one collection — the console delete. `user_id` is part of
/// the key, the way the storage API addresses an object.
pub async fn delete_storage_object(
    pool: &sqlx::PgPool,
    collection: &str,
    key: &str,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let done =
        sqlx::query("DELETE FROM storage WHERE collection = $1 AND key = $2 AND user_id = $3;")
            .bind(collection)
            .bind(key)
            .bind(user_id)
            .execute(pool)
            .await?;
    Ok(done.rows_affected() > 0)
}

/// One wallet ledger entry, addressed by its own id and the account it belongs to.
/// `user_id` is part of the match so an operator cannot delete a ledger entry by
/// guessing an id that belongs to another account.
pub async fn delete_wallet_ledger(
    pool: &sqlx::PgPool,
    ledger_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM wallet_ledger WHERE id = $1 AND user_id = $2;")
        .bind(ledger_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// One notification, addressed by its own id. The console deletes a notification
/// outright — mirroring reference, the row is not scoped by recipient, so an operator
/// can remove a notification by id alone.
pub async fn delete_notification(pool: &sqlx::PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM notification WHERE id = $1;")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() > 0)
}
