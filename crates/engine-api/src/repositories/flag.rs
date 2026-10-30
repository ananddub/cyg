#![allow(unused_imports, dead_code)]

use super::db::{Queries, WriteResult, affected, params};
use super::models::{FeatureFlag, FlagOverride, FlagVariant};
use crate::db::{DbError, Key};
use serde_json::Value;
use uuid::Uuid;

impl Queries {
    pub async fn list_flags(&self) -> Result<Vec<FeatureFlag>, DbError> {
        self.all("SELECT * FROM feature_flag ORDER BY id ASC", params!())
            .await
    }

    pub async fn get_flag(&self, flag_key: &str) -> Result<Option<FeatureFlag>, DbError> {
        self.opt(
            "SELECT * FROM type::record('feature_flag', $id) LIMIT 1",
            params!(id = flag_key),
        )
        .await
    }

    /// Creates a flag, reporting `false` when the key is already taken.
    pub async fn create_flag(&self, row: &FeatureFlag) -> Result<bool, DbError> {
        match self
            .create("feature_flag", Key::String(row.id.clone()), row)
            .await
        {
            Ok(_) => Ok(true),
            Err(err) if err.is_unique() => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub async fn update_flag(&self, row: &FeatureFlag) -> Result<(), DbError> {
        self.put_row("feature_flag", &row.id, row).await
    }

    pub async fn delete_flag(&self, flag_key: &str) -> Result<WriteResult, DbError> {
        affected(
            self,
            "DELETE type::record('feature_flag', $id) RETURN BEFORE",
            params!(id = flag_key),
        )
        .await
    }

    /// Every arm of every flag, in the one read a flag list needs. Arms come
    /// back ordered by flag then name, so the first of a flag is its control.
    pub async fn list_variants(&self) -> Result<Vec<FlagVariant>, DbError> {
        self.all(
            "SELECT * FROM flag_variant ORDER BY flag_key ASC, name ASC",
            params!(),
        )
        .await
    }

    pub async fn get_variant(
        &self,
        flag_key: &str,
        name: &str,
    ) -> Result<Option<FlagVariant>, DbError> {
        self.opt(
            "SELECT * FROM type::record('flag_variant', $id) LIMIT 1",
            params!(id = format!("{flag_key}:{name}")),
        )
        .await
    }

    /// One flag's arms, ordered by name so the first is its control.
    pub async fn arms_of(&self, flag_key: &str) -> Result<Vec<FlagVariant>, DbError> {
        self.all(
            "SELECT * FROM flag_variant WHERE flag_key = $flag ORDER BY name ASC",
            params!(flag = flag_key),
        )
        .await
    }

    /// Adds an arm, reporting `false` when the flag already has one by that
    /// name. A flag with a second control arm is a flag nobody can read.
    pub async fn create_variant(&self, row: &FlagVariant) -> Result<bool, DbError> {
        match self
            .create("flag_variant", Key::String(row.id.clone()), row)
            .await
        {
            Ok(_) => Ok(true),
            Err(err) if err.is_unique() => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub async fn update_variant(&self, row: &FlagVariant) -> Result<(), DbError> {
        self.put_row("flag_variant", &row.id, row).await
    }

    pub async fn delete_variant(&self, flag_key: &str, name: &str) -> Result<WriteResult, DbError> {
        affected(
            self,
            "DELETE type::record('flag_variant', $id) RETURN BEFORE",
            params!(id = format!("{flag_key}:{name}")),
        )
        .await
    }

    pub async fn list_overrides(&self, flag_key: &str) -> Result<Vec<FlagOverride>, DbError> {
        self.all(
            "SELECT * FROM flag_override WHERE flag_key = $flag ORDER BY id ASC",
            params!(flag = flag_key),
        )
        .await
    }

    /// Everything this account is pinned to, across every flag: the one read a
    /// resolved flag list needs.
    pub async fn overrides_of(&self, user_id: Uuid) -> Result<Vec<FlagOverride>, DbError> {
        self.all(
            "SELECT * FROM flag_override WHERE user_id = $user ORDER BY id ASC",
            params!(user = user_id),
        )
        .await
    }

    pub async fn get_override(
        &self,
        flag_key: &str,
        user_id: Uuid,
    ) -> Result<Option<FlagOverride>, DbError> {
        self.opt(
            "SELECT * FROM type::record('flag_override', $id) LIMIT 1",
            params!(id = format!("{flag_key}:{user_id}")),
        )
        .await
    }

    pub async fn upsert_override(&self, row: &FlagOverride) -> Result<(), DbError> {
        self.put_row("flag_override", &row.id, row).await
    }

    pub async fn delete_override(
        &self,
        flag_key: &str,
        user_id: Uuid,
    ) -> Result<WriteResult, DbError> {
        affected(
            self,
            "DELETE type::record('flag_override', $id) RETURN BEFORE",
            params!(id = format!("{flag_key}:{user_id}")),
        )
        .await
    }

    /// Drops an account's whole set of arms and exemptions with its flag, so a
    /// deleted experiment cannot keep assigning a key nothing defines anymore.
    pub async fn delete_flag_parts(&self, flag_key: &str) -> Result<(), DbError> {
        self.exec(
            "DELETE flag_variant WHERE flag_key = $flag; \
             DELETE flag_override WHERE flag_key = $flag",
            params!(flag = flag_key),
        )
        .await
    }
}

impl Queries {
    /// Writes a row over itself and leaves the id alone, which is the record
    /// key: restating it in `CONTENT` is a field definition waiting to
    /// disagree with the key that already holds it.
    async fn put_row<T: serde::Serialize>(
        &self,
        table: &str,
        id: &str,
        row: &T,
    ) -> Result<(), DbError> {
        let mut data = serde_json::to_value(row)?;
        if let Value::Object(map) = &mut data {
            map.remove("id");
        }
        self.exec(
            &format!("UPSERT type::record('{table}', $id) CONTENT $data"),
            params!(id = id, data = data),
        )
        .await
    }
}
