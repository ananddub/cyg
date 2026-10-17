//! The export side of the account document: everything the console holds about one
//! account, assembled from its tables.

use axum::Json;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::console::account::{
    AccountExport, AccountNoteRow, ExportedAccount, ExportedDevice, ExportedEdge, ExportedStorage,
    WalletLedgerRow,
};
use crate::error::V2Error;
use crate::repositories::console::{ListStorageRow, ListWalletLedgerParams};
use crate::repositories::console_mutations::ListUserEdgesRow;
use crate::ApiState;

/// How much of an account's ledger an export carries. A document an operator
/// inspects is not a dump of an unbounded table.
const EXPORT_ROWS: i64 = 1000;

impl super::ConsoleService {
    /// Everything the console holds about one account, in the shape
    /// [`import_account`](Self::import_account) reads back.
    pub async fn export_account(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<AccountExport>, V2Error> {
        // An export carries every private thing the console holds about an account,
        // so reading one is a write-grade privilege.
        claims.require_write()?;
        let id = super::account_id(id)?;
        let queries = super::queries(state)?;
        let user = queries.get_user_by_id(id).await?;
        let devices = queries.list_user_devices(id).await?;
        let notes = queries.list_user_notes(id).await?;
        let ledger = queries
            .list_wallet_ledger(&ListWalletLedgerParams {
                user_id: id,
                limit: EXPORT_ROWS,
                offset: 0,
            })
            .await?;
        let storage = queries.list_storage_for_user(id).await?;
        let edges = queries.list_user_edges(id).await?;

        Ok(Json(AccountExport {
            account: ExportedAccount::from(user),
            devices: devices
                .into_iter()
                .map(|device| ExportedDevice {
                    id: device.id,
                    provider: device.provider,
                })
                .collect(),
            notes: notes.into_iter().map(AccountNoteRow::from).collect(),
            wallet_ledger: ledger.into_iter().map(WalletLedgerRow::from).collect(),
            storage: storage.into_iter().map(exported_storage).collect(),
            edges: edges.into_iter().map(exported_edge).collect(),
        }))
    }
}

/// The stored JSON text as the export's `value`. The column is `jsonb`, so a parse
/// failure is impossible; falling back to the raw text keeps the object in the
/// export rather than dropping it.
fn exported_storage(row: ListStorageRow) -> ExportedStorage {
    let value = match serde_json::from_str(&row.value) {
        Ok(value) => value,
        Err(_) => serde_json::Value::String(row.value.clone()),
    };
    ExportedStorage {
        collection: row.collection,
        key: row.object_key,
        value,
        version: row.version,
        read: row.read,
        write: row.write,
        create_time: row.create_time,
        update_time: row.update_time,
    }
}

fn exported_edge(row: ListUserEdgesRow) -> ExportedEdge {
    ExportedEdge {
        destination_id: row.destination_id,
        state: row.state,
        position: row.position,
        metadata: row.metadata,
    }
}
