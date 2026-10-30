use super::*;

/// The record id of one published version of one rule set.
pub fn rules_definition_id(key: &str, version: i64) -> String {
    format!("{key}:{version}")
}

impl Queries {
    pub async fn list_rules(&self) -> Result<Vec<RulesDefinition>, DbError> {
        self.all(
            "SELECT * FROM rules_definition ORDER BY key ASC, version DESC",
            params!(),
        )
        .await
    }

    pub async fn list_rule_versions(&self, key: &str) -> Result<Vec<RulesDefinition>, DbError> {
        self.all(
            "SELECT * FROM rules_definition WHERE key = $key ORDER BY version DESC",
            params!(key = key),
        )
        .await
    }

    pub async fn get_rule(
        &self,
        key: &str,
        version: i64,
    ) -> Result<Option<RulesDefinition>, DbError> {
        self.opt(
            "SELECT * FROM type::record('rules_definition', $id) LIMIT 1",
            params!(id = rules_definition_id(key, version)),
        )
        .await
    }

    pub async fn active_rule(&self, key: &str) -> Result<Option<RulesDefinition>, DbError> {
        self.opt(
            "SELECT * FROM rules_definition WHERE key = $key AND is_active = true LIMIT 1",
            params!(key = key),
        )
        .await
    }

    pub async fn latest_rule_version(&self, key: &str) -> Result<i64, DbError> {
        let rows: Vec<LatestRow> = self
            .all(
                "SELECT version FROM rules_definition WHERE key = $key ORDER BY version DESC LIMIT 1",
                params!(key = key),
            )
            .await?;
        Ok(rows.first().map(|row| row.version).unwrap_or(0))
    }

    /// Creates one version, reporting `false` when it already exists.
    pub async fn create_rule(&self, row: &RulesDefinition) -> Result<bool, DbError> {
        match self
            .create("rules_definition", Key::String(row.id.clone()), row)
            .await
        {
            Ok(_) => Ok(true),
            Err(err) if err.is_unique() => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// Activates one version and retires every other version of the key in a
    /// single transaction, so readers never see two active versions.
    pub async fn activate_rule(&self, key: &str, version: i64) -> Result<(), DbError> {
        self.exec(
            "BEGIN TRANSACTION; \
             UPDATE rules_definition SET is_active = false WHERE key = $key AND is_active = true; \
             UPDATE type::record('rules_definition', $id) SET is_active = true; \
             COMMIT;",
            params!(key = key, id = rules_definition_id(key, version)),
        )
        .await
    }

    pub async fn delete_rule_versions(&self, key: &str) -> Result<WriteResult, DbError> {
        affected(
            self,
            "DELETE rules_definition WHERE key = $key RETURN BEFORE",
            params!(key = key),
        )
        .await
    }

    /// Lists the current stats of one player, the context a rule evaluates
    /// against during a dry run.
    pub async fn player_stat_context(&self, user_id: Uuid) -> Result<Value, DbError> {
        let rows: Vec<StatRow> = self
            .all(
                "SELECT stat, value FROM player_stat WHERE user_id = $user",
                params!(user = user_id),
            )
            .await?;
        let mut map = serde_json::Map::new();
        for row in rows {
            map.insert(row.stat, row.value);
        }
        Ok(Value::Object(map))
    }
}

#[derive(serde::Deserialize)]
struct LatestRow {
    version: i64,
}

#[derive(serde::Deserialize)]
struct StatRow {
    stat: String,
    value: Value,
}
