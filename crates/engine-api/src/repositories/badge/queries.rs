use super::*;

impl Queries {
    pub async fn list_badge_definitions(&self) -> Result<Vec<BadgeDefinition>, DbError> {
        self.all("SELECT * FROM badge_definition ORDER BY id ASC", params!())
            .await
    }

    pub async fn get_badge_definition(
        &self,
        badge_key: &str,
    ) -> Result<Option<BadgeDefinition>, DbError> {
        self.opt(
            "SELECT * FROM type::record('badge_definition', $id) LIMIT 1",
            params!(id = badge_key),
        )
        .await
    }

    /// Creates a definition, reporting `false` when the key is already taken.
    pub async fn create_badge_definition(&self, row: &BadgeDefinition) -> Result<bool, DbError> {
        match self
            .create("badge_definition", Key::String(row.id.clone()), row)
            .await
        {
            Ok(_) => Ok(true),
            Err(err) if err.is_unique() => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub async fn update_badge_definition(&self, row: &BadgeDefinition) -> Result<(), DbError> {
        let mut data = serde_json::to_value(row)?;
        if let Value::Object(map) = &mut data {
            map.remove("id");
        }
        self.exec(
            "UPSERT type::record('badge_definition', $id) CONTENT $data",
            params!(id = row.id.clone(), data = data),
        )
        .await
    }

    pub async fn delete_badge_definition(&self, badge_key: &str) -> Result<(), DbError> {
        self.exec(
            "DELETE type::record('badge_definition', $id)",
            params!(id = badge_key),
        )
        .await
    }
}
