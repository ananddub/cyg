//! The writes that change an account's standing: the ban, the note, and the delete.

use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::account::{AddNoteRequest, BanAccountRequest};
use crate::error::V2Error;
use crate::repositories::console_mutations::{
    InsertUserNoteAnonymousParams, InsertUserNoteParams,
};
use crate::ApiState;

impl super::ConsoleService {
    /// Ban an account: `disable_time` becomes now, which is what every other
    /// surface reads as "disabled". The reason lives in the trail, not on the row.
    pub async fn ban_account(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
        body: BanAccountRequest,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let id = super::account_id(id)?;
        let queries = super::queries(state)?;
        // Read first, so an id that names nothing is a 404 rather than a silent
        // no-op that looks like a successful ban.
        let user = queries.get_user_by_id(id).await?;
        queries.set_account_disabled(Utc::now(), id).await?;

        let reason = body.reason.trim();
        let message = if reason.is_empty() {
            format!("Banned {}.", user.username)
        } else {
            format!("Banned {}: {reason}", user.username)
        };
        super::audit(state, claims, "account.ban", "account", Some(id), &message).await;
        Ok(Json(Empty {}))
    }

    /// Lift a ban: the epoch means "not disabled", the same spelling the schema uses
    /// for a group that was never deleted or a purchase that was never refunded.
    pub async fn unban_account(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let id = super::account_id(id)?;
        let queries = super::queries(state)?;
        let user = queries.get_user_by_id(id).await?;
        queries.set_account_disabled(super::never(), id).await?;
        super::audit(
            state,
            claims,
            "account.unban",
            "account",
            Some(id),
            &format!("Unbanned {}.", user.username),
        )
        .await;
        Ok(Json(Empty {}))
    }

    /// Delete an account for good. The tombstone goes first so the id cannot be
    /// recreated; the row then takes its devices, notes, storage, messages and edges
    /// with it and leaves its purchases pointing at the nil id.
    pub async fn delete_account(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_admin()?;
        let id = super::account_id(id)?;
        let queries = super::queries(state)?;
        let user = queries.get_user_by_id(id).await?;
        queries.create_tombstone(id).await?;
        queries.delete_account(id).await?;
        super::audit(
            state,
            claims,
            "account.delete",
            "account",
            Some(id),
            &format!("Deleted account {}.", user.username),
        )
        .await;
        Ok(Json(Empty {}))
    }

    pub async fn add_account_note(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
        body: AddNoteRequest,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let id = super::account_id(id)?;
        let note = body.note.trim();
        if note.is_empty() {
            return Err(V2Error::invalid_argument("A note cannot be empty."));
        }
        let queries = super::queries(state)?;
        queries.get_user_by_id(id).await?;
        // `create_id` is the operator who wrote it. The bootstrap superadmin has no
        // `console_user` row to name, so its notes are written without an author
        // rather than with the nil id the audit trail uses.
        match claims.0.uuid() {
            Some(author) => {
                queries
                    .insert_user_note(&InsertUserNoteParams {
                        id: Uuid::new_v4(),
                        user_id: id,
                        note: note.to_owned(),
                        create_id: author,
                    })
                    .await?;
            }
            None => {
                queries
                    .insert_user_note_anonymous(&InsertUserNoteAnonymousParams {
                        id: Uuid::new_v4(),
                        user_id: id,
                        note: note.to_owned(),
                    })
                    .await?;
            }
        }
        super::audit(
            state,
            claims,
            "account.note.create",
            "account",
            Some(id),
            "Added an account note",
        )
        .await;
        Ok(Json(Empty {}))
    }

    pub async fn delete_account_note(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
        note_id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let id = super::account_id(id)?;
        let note_id = super::id(note_id, "note")?;
        let queries = super::queries(state)?;
        if queries.delete_user_note(note_id, id).await?.rows_affected() == 0 {
            return Err(V2Error::not_found("Note not found."));
        }
        super::audit(
            state,
            claims,
            "account.note.delete",
            "account",
            Some(id),
            "Deleted an account note",
        )
        .await;
        Ok(Json(Empty {}))
    }
}
