use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::account::{AddNoteRequest, BanAccountRequest};
use crate::error::V2Error;
use crate::repositories::console_mutations::{InsertUserNoteAnonymousParams, InsertUserNoteParams};

impl super::ConsoleService {
    pub async fn ban_account(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
        body: BanAccountRequest,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let id = super::account_id(id)?;
        let queries = super::queries(state).await?;
        let user = queries.get_user_by_id(id).await?;
        queries.set_account_disabled(Utc::now(), id).await?;

        let reason = body.reason.trim();
        let message = if reason.is_empty() {
            format!("Banned {}.", user.username)
        } else {
            format!("Banned {}: {reason}", user.username)
        };

        state.sessions.remove_all(id);
        let kicked =
            crate::services::realtime::RealtimeSessionHub::global().kick_user(id, &message);

        let message = if kicked > 0 {
            format!("{message} Kicked {kicked} live session(s).")
        } else {
            message
        };
        super::audit(state, claims, "account.ban", "account", Some(id), &message).await;
        Ok(Json(Empty {}))
    }

    pub async fn unban_account(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let id = super::account_id(id)?;
        let queries = super::queries(state).await?;
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

    pub async fn delete_account(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_admin()?;
        let id = super::account_id(id)?;
        let queries = super::queries(state).await?;
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
        let queries = super::queries(state).await?;
        queries.get_user_by_id(id).await?;
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
        let queries = super::queries(state).await?;
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
