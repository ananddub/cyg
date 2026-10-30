use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::account::AccountExport;
use crate::error::V2Error;
use crate::repositories::console::ListStorageRow;
use crate::repositories::console_mutations::{
    ImportAccountParams, ImportDeviceParams, ImportLedgerParams, ImportNoteParams, ListUserEdgesRow,
};
use crate::repositories::models::Users;
use crate::services::deps::q;

use super::conflict::import_conflict;

impl super::ConsoleService {
    pub async fn import_account_by_id(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
        game: Option<Uuid>,
        body: AccountExport,
    ) -> Result<Json<Empty>, V2Error> {
        let id = super::account_id(id)?;
        if body.account.id != id {
            return Err(V2Error::invalid_argument(
                "The account id in the body must match the id in the path.",
            ));
        }
        self.import_account(state, claims, game, body).await
    }

    pub async fn import_account(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        game: Option<Uuid>,
        body: AccountExport,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_admin()?;
        let account = body.account;
        if account.id.is_nil() {
            return Err(V2Error::invalid_argument(
                "The imported account needs an id.",
            ));
        }
        if account.username.trim().is_empty() {
            return Err(V2Error::invalid_argument(
                "The imported account needs a username.",
            ));
        }
        let id = account.id;
        let username = account.username.clone();

        let user = Users {
            game_id: None,
            id,
            username: account.username,
            display_name: account.display_name,
            avatar_url: account.avatar_url,
            lang_tag: account.lang_tag,
            location: account.location,
            timezone: account.timezone,
            metadata: account.metadata,
            wallet: account.wallet,
            email: account.email,
            password: None,
            facebook_id: account.facebook_id,
            google_id: account.google_id,
            gamecenter_id: account.gamecenter_id,
            steam_id: account.steam_id,
            custom_id: account.custom_id,
            edge_count: account.edge_count,
            create_time: account.create_time,
            update_time: account.update_time,
            verify_time: account.verify_time,
            disable_time: account.disable_time,
            facebook_instant_game_id: account.facebook_instant_game_id,
            apple_id: account.apple_id,
        };

        let params = ImportAccountParams {
            account: user,
            devices: body
                .devices
                .into_iter()
                .map(|device| ImportDeviceParams {
                    id: device.id,
                    provider: device.provider,
                })
                .collect(),
            notes: body
                .notes
                .into_iter()
                .map(|note| ImportNoteParams {
                    id: note.id,
                    note: note.note,
                    create_time: note.create_time,
                    update_time: note.update_time,
                })
                .collect(),
            wallet_ledger: body
                .wallet_ledger
                .into_iter()
                .map(|entry| ImportLedgerParams {
                    id: entry.id,
                    changeset: entry.changeset,
                    metadata: entry.metadata,
                    create_time: entry.create_time,
                    update_time: entry.update_time,
                })
                .collect(),
            storage: body
                .storage
                .into_iter()
                .map(|object| ListStorageRow {
                    collection: object.collection,
                    object_key: object.key,
                    user_id: id,
                    value: object.value.to_string(),
                    version: object.version,
                    read: object.read,
                    write: object.write,
                    create_time: object.create_time,
                    update_time: object.update_time,
                })
                .collect(),
            edges: body
                .edges
                .into_iter()
                .map(|edge| ListUserEdgesRow {
                    destination_id: edge.destination_id,
                    state: edge.state,
                    position: edge.position,
                    metadata: edge.metadata,
                })
                .collect(),
        };

        q(state)
            .await?
            .import_account_bundle(&params)
            .await
            .map_err(import_conflict)?;
        crate::services::games::tenant::stamp_user_as(state, id, game).await;
        super::audit(
            state,
            claims,
            "account.import",
            "account",
            Some(id),
            &format!("Imported account {username}."),
        )
        .await;
        Ok(Json(Empty {}))
    }
}
