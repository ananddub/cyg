use std::collections::{BTreeMap, HashMap};

use sha2::{Digest, Sha256};

use super::error::DbError;
use super::exec::Datastore;

/// One numbered SurrealQL step applied after the generated base DDL. Name the
/// backing file `<version>_<slug>.surql`; a step that has already been applied
/// is immutable — changing its checksum aborts the next boot, so add a new
/// step instead.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: u64,
    pub name: &'static str,
    pub sql: &'static str,
}

impl Migration {
    pub fn checksum(&self) -> String {
        hex::encode(Sha256::digest(self.sql.as_bytes()))
    }
}

/// The compiled-in history, applied in version order. Fresh databases get the
/// generated DDL first, so a step here must only contain what an existing
/// install still needs.
pub static MIGRATIONS: &[Migration] = &[];

#[derive(Debug, Clone)]
pub struct Applied {
    pub version: u64,
    pub name: String,
    pub checksum: String,
}

#[derive(serde::Deserialize)]
struct AppliedRow {
    version: u64,
    name: String,
    checksum: String,
}

pub async fn applied(db: &Datastore) -> Result<Vec<Applied>, DbError> {
    let rows: Vec<AppliedRow> = db
        .query("SELECT version, name, checksum FROM schema_version ORDER BY version")
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| Applied {
            version: r.version,
            name: r.name,
            checksum: r.checksum,
        })
        .collect())
}

pub async fn pending(db: &Datastore, list: &[Migration]) -> Result<Vec<String>, DbError> {
    let seen: HashMap<u64, String> = applied(db)
        .await?
        .into_iter()
        .map(|a| (a.version, a.checksum))
        .collect();
    Ok(list
        .iter()
        .filter(|m| !seen.contains_key(&m.version))
        .map(|m| m.name.to_string())
        .collect())
}

pub async fn apply_migrations(db: &Datastore, list: &[Migration]) -> Result<Vec<String>, DbError> {
    let mut steps = ordered(list)?;
    let seen: HashMap<u64, String> = applied(db)
        .await?
        .into_iter()
        .map(|a| (a.version, a.checksum))
        .collect();
    for (version, checksum) in &seen {
        if let Some(m) = list.iter().find(|m| m.version == *version) {
            if m.checksum() != *checksum {
                return Err(DbError::Query(format!(
                    "migration {version} ({}) changed after it was applied",
                    m.name
                )));
            }
        } else {
            tracing::warn!("schema_version {version} was applied by a newer build; skipping");
        }
    }
    steps.retain(|m| !seen.contains_key(&m.version));
    let mut applied_now = Vec::new();
    for m in steps {
        let script = format!(
            "BEGIN TRANSACTION; \
             {} \
             UPSERT type::record('schema_version', <int>$version) CONTENT {{ \
                 version: $version, name: $name, checksum: $checksum, applied_at: time::now() \
             }}; \
             COMMIT TRANSACTION;",
            m.sql
        );
        let params = BTreeMap::from([
            ("version".to_string(), serde_json::json!(m.version)),
            ("name".to_string(), serde_json::json!(m.name)),
            ("checksum".to_string(), serde_json::json!(m.checksum())),
        ]);
        db.exec_params(&script, &params).await?;
        tracing::info!("[migrations] applied {} ({})", m.version, m.name);
        applied_now.push(m.name.to_string());
    }
    Ok(applied_now)
}

fn ordered(list: &[Migration]) -> Result<Vec<&Migration>, DbError> {
    let mut steps: Vec<&Migration> = list.iter().collect();
    steps.sort_by_key(|m| m.version);
    if steps.windows(2).any(|w| w[0].version == w[1].version) {
        return Err(DbError::Query(
            "two migrations share a version number".into(),
        ));
    }
    if steps
        .iter()
        .enumerate()
        .any(|(i, m)| m.version != i as u64 + 1)
    {
        return Err(DbError::Query(
            "migration versions must be 1..n without gaps".into(),
        ));
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{KvBudget, schema};

    async fn db() -> Datastore {
        let db = Datastore::open("mem://", "mig", "mig", None, None, KvBudget::default())
            .await
            .expect("the embedded store opens");
        schema::ensure_schema(&db)
            .await
            .expect("the base DDL applies");
        db
    }

    #[tokio::test]
    async fn a_second_boot_applies_nothing() {
        let db = db().await;
        let list = &[Migration {
            version: 1,
            name: "one",
            sql: "CREATE probe SET x = 1;",
        }];
        assert_eq!(
            apply_migrations(&db, list).await.unwrap(),
            vec!["one".to_string()]
        );
        assert!(apply_migrations(&db, list).await.unwrap().is_empty());
        assert!(pending(&db, list).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_bad_step_rolls_back_and_writes_no_version_row() {
        let db = db().await;
        let list = &[
            Migration {
                version: 1,
                name: "good",
                sql: "CREATE probe SET x = 1;",
            },
            Migration {
                version: 2,
                name: "bad",
                sql: "CREATE doomed SET x = 1; THROW 'boom';",
            },
        ];
        apply_migrations(&db, list)
            .await
            .expect_err("the throw aborts the step");
        let err = db
            .query::<serde_json::Value>("SELECT VALUE id FROM doomed")
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("doomed"),
            "the doomed insert must roll back: {err}"
        );
        let versions: Vec<u64> = db
            .query("SELECT VALUE version FROM schema_version")
            .await
            .unwrap();
        assert_eq!(versions, vec![1], "only the good step is recorded");
        assert_eq!(pending(&db, list).await.unwrap(), vec!["bad".to_string()]);
    }

    #[tokio::test]
    async fn editing_an_applied_step_aborts_the_next_boot() {
        let db = db().await;
        let done = [Migration {
            version: 1,
            name: "one",
            sql: "CREATE a SET x = 1;",
        }];
        apply_migrations(&db, &done).await.unwrap();
        let drifted = [Migration {
            version: 1,
            name: "one",
            sql: "CREATE a SET x = 2;",
        }];
        let err = apply_migrations(&db, &drifted)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("changed after it was applied"), "{err}");
    }

    #[tokio::test]
    async fn a_version_from_a_newer_build_is_skipped_not_refused() {
        let db = db().await;
        let ahead = [Migration {
            version: 1,
            name: "future",
            sql: "CREATE q SET x = 1;",
        }];
        apply_migrations(&db, &ahead).await.unwrap();
        let behind: &[Migration] = &[];
        assert!(apply_migrations(&db, behind).await.unwrap().is_empty());
    }

    #[test]
    fn the_default_history_is_ordered_and_gapless() {
        assert!(ordered(MIGRATIONS).is_ok());
    }

    #[test]
    fn a_duplicate_version_is_refused() {
        let list = [
            Migration {
                version: 1,
                name: "a",
                sql: "",
            },
            Migration {
                version: 1,
                name: "b",
                sql: "",
            },
        ];
        assert!(ordered(&list).is_err());
    }
}
