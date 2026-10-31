use somnia::SurrealSchema;

use super::error::DbError;
use super::exec::Datastore;
use crate::repositories::models::*;

macro_rules! schema_ddl {
    ($($record:ty),* $(,)?) => {{
        let mut ddl: Vec<String> = Vec::new();
        $(
            ddl.push(<$record as SurrealSchema>::define_table().to_string());
            ddl.extend(<$record as SurrealSchema>::define_fields().iter().map(|field| typed_field(field)));
            ddl.extend(<$record as SurrealSchema>::define_indexes().iter().map(|index| index.to_string()));
        )*
        ddl
    }};
}

pub fn ddl() -> Vec<String> {
    schema_ddl![
        BadgeDefinition,
        ConsoleAclTemplate,
        ConsoleAuditLog,
        ConsoleUser,
        ContentReport,
        DeliveryAttempt,
        DesyncReport,
        FeatureFlag,
        FlagOverride,
        FlagVariant,
        GameConfigVersion,
        GameMaps,
        Games,
        GameplayEvent,
        GroupEdge,
        Groups,
        IdempotencyKey,
        ItemDefinition,
        PlayerInventoryItem,
        PlayerQuest,
        PlayerStat,
        PlayerQuota,
        Leaderboard,
        LeaderboardRecord,
        MarketListing,
        MatchHistory,
        Message,
        Notification,
        NotificationPreference,
        Purchase,
        QuestDefinition,
        RoomModules,
        RoomPersistentSnapshot,
        RoomRegistry,
        RulesDefinition,
        SchemaVersion,
        Season,
        Setting,
        Storage,
        StorageIndex,
        Subscription,
        UserDevice,
        UserEdge,
        UserInfraction,
        UserTombstone,
        Users,
        UsersNotes,
        WalletLedger,
        Wager,
        WebhookEndpoint,
        WebhookDelivery,
    ]
}

/// Rewrites a derived `DEFINE FIELD` so its type describes the stored value
/// rather than what a Rust field could hold: this store was schemaless, every
/// write is JSON, so UUIDs and timestamps are strings and a column may be
/// absent or null. `option<null | T>` accepts all three while still typing the
/// value whenever one is present, so rows written before the types existed
/// stay writable.
fn typed_field(statement: &str) -> String {
    let Some((head, rest)) = statement.split_once(" TYPE ") else {
        return statement.to_string();
    };
    let (ty, tail) = match rest.split_once(' ') {
        Some((ty, tail)) => (ty, format!(" {tail}")),
        None => (rest.trim_end_matches(';'), ";".to_string()),
    };
    format!("{head} TYPE {}{tail}", stored_type(ty))
}

fn stored_type(ty: &str) -> String {
    let ty = ty
        .replace("uuid", "uuid | string")
        .replace("datetime", "datetime | string")
        .replace("object", "any");
    if ty == "any" || ty == "option<any>" {
        return "any".to_string();
    }
    let inner = ty
        .strip_prefix("option<")
        .and_then(|inner| inner.strip_suffix('>'))
        .unwrap_or(&ty);
    format!("option<null | {inner}>")
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
            51
        );
        assert!(ddl.iter().all(|line| !line.contains("SCHEMAFULL")));
        for line in ddl.iter().filter(|line| line.starts_with("DEFINE INDEX")) {
            assert!(line.contains("IF NOT EXISTS"), "{line}");
        }
    }

    #[test]
    fn every_field_is_typed_and_tolerates_the_stored_shape() {
        let ddl = ddl();
        let fields: Vec<_> = ddl
            .iter()
            .filter(|line| line.starts_with("DEFINE FIELD"))
            .collect();
        assert!(!fields.is_empty());
        for line in fields {
            assert!(line.contains("IF NOT EXISTS"), "{line}");
            let (_, ty) = line.split_once(" TYPE ").expect("a typed field");
            assert!(
                ty.starts_with("option<null | ") || ty.starts_with("any"),
                "{line}"
            );
        }
    }

    #[test]
    fn a_type_keeps_its_clauses_and_its_store_shape() {
        assert_eq!(
            typed_field(
                "DEFINE FIELD IF NOT EXISTS name ON TABLE groups TYPE string PERMISSIONS FULL;"
            ),
            "DEFINE FIELD IF NOT EXISTS name ON TABLE groups TYPE option<null | string> PERMISSIONS FULL;"
        );
        assert_eq!(
            typed_field(
                "DEFINE FIELD IF NOT EXISTS game_id ON TABLE users TYPE uuid PERMISSIONS FULL;"
            ),
            "DEFINE FIELD IF NOT EXISTS game_id ON TABLE users TYPE option<null | uuid | string> PERMISSIONS FULL;"
        );
        assert_eq!(
            typed_field(
                "DEFINE FIELD IF NOT EXISTS tags ON TABLE users TYPE option<array<string>> PERMISSIONS FULL;"
            ),
            "DEFINE FIELD IF NOT EXISTS tags ON TABLE users TYPE option<null | array<string>> PERMISSIONS FULL;"
        );
        assert_eq!(
            typed_field(
                "DEFINE FIELD IF NOT EXISTS metadata ON TABLE users TYPE any PERMISSIONS FULL;"
            ),
            "DEFINE FIELD IF NOT EXISTS metadata ON TABLE users TYPE any PERMISSIONS FULL;"
        );
        assert_eq!(
            typed_field(
                "DEFINE FIELD IF NOT EXISTS created_at ON TABLE users TYPE datetime PERMISSIONS FULL;"
            ),
            "DEFINE FIELD IF NOT EXISTS created_at ON TABLE users TYPE option<null | datetime | string> PERMISSIONS FULL;"
        );
        assert_eq!(
            typed_field("DEFINE FIELD IF NOT EXISTS name ON TABLE groups TYPE string;"),
            "DEFINE FIELD IF NOT EXISTS name ON TABLE groups TYPE option<null | string>;"
        );
        assert_eq!(
            typed_field("DEFINE FIELD IF NOT EXISTS metadata ON TABLE users TYPE object;"),
            "DEFINE FIELD IF NOT EXISTS metadata ON TABLE users TYPE any;"
        );
    }
}
