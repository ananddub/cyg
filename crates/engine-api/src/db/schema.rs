use somnia::SurrealSchema;

use super::error::DbError;
use super::exec::Datastore;
use crate::repositories::models::*;

macro_rules! schema_ddl {
    ($($record:ty),* $(,)?) => {{
        let mut ddl: Vec<&'static str> = Vec::new();
        $(
            ddl.push(<$record as SurrealSchema>::define_table());
            ddl.extend_from_slice(<$record as SurrealSchema>::define_indexes());
        )*
        ddl
    }};
}

pub fn ddl() -> Vec<&'static str> {
    schema_ddl![
        ConsoleAclTemplate,
        ConsoleAuditLog,
        ConsoleUser,
        GameMaps,
        Games,
        GroupEdge,
        Groups,
        IdempotencyKey,
        ItemDefinition,
        PlayerInventoryItem,
        PlayerQuest,
        PlayerStat,
        Leaderboard,
        LeaderboardRecord,
        MarketListing,
        Message,
        Notification,
        Purchase,
        QuestDefinition,
        RoomModules,
        RoomPersistentSnapshot,
        SchemaVersion,
        Season,
        Setting,
        Storage,
        StorageIndex,
        Subscription,
        UserDevice,
        UserEdge,
        UserTombstone,
        Users,
        UsersNotes,
        WalletLedger,
        Wager,
    ]
}

pub async fn ensure_schema(db: &Datastore) -> Result<(), DbError> {
    db.apply(ddl()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_table_is_schemaless_and_every_index_is_guarded() {
        let ddl = ddl();
        assert_eq!(
            ddl.iter()
                .filter(|line| line.starts_with("DEFINE TABLE"))
                .count(),
            34
        );
        assert!(ddl.iter().all(|line| !line.contains("SCHEMAFULL")));
        for line in ddl.iter().filter(|line| line.starts_with("DEFINE INDEX")) {
            assert!(line.contains("IF NOT EXISTS"), "{line}");
        }
    }
}
