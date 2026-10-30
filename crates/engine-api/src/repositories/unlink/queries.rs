use super::*;

impl Queries {
    async fn guarded_unlink(
        &self,
        id: Uuid,
        column: &str,
        assignment: &str,
    ) -> Result<Uuid, DbError> {
        let user: Users = self
            .one(
                "SELECT * FROM type::record('users', <uuid>$id) LIMIT 1",
                params!(id = id.to_string()),
            )
            .await?;
        let other_identity = identity_columns(&user)
            .iter()
            .any(|(name, present)| *name != column && *present);
        let device: Option<UserDevice> = self
            .opt(
                "SELECT * FROM user_device WHERE user_id = $user_id LIMIT 1",
                params!(user_id = id.to_string()),
            )
            .await?;
        if !other_identity && device.is_none() {
            return Err(DbError::NotFound);
        }
        let updated: Users = self
            .one(
                &format!(
                    "UPDATE type::record('users', <uuid>$id) \
                     SET {assignment}, update_time = $now RETURN AFTER"
                ),
                params!(
                    now = time::encode(Utc::now()),
                    id = id.to_string(),
                    unset = Value::Null,
                ),
            )
            .await?;
        Ok(updated.id)
    }

    pub async fn unlink_email(&self, id: Uuid) -> Result<Uuid, DbError> {
        self.guarded_unlink(id, "email", "email = $unset, password = $unset")
            .await
    }

    pub async fn unlink_custom_id(&self, id: Uuid) -> Result<Uuid, DbError> {
        self.guarded_unlink(id, "custom_id", "custom_id = $unset")
            .await
    }

    pub async fn unlink_apple(&self, id: Uuid) -> Result<Uuid, DbError> {
        self.guarded_unlink(id, "apple_id", "apple_id = $unset")
            .await
    }

    pub async fn unlink_google(&self, id: Uuid) -> Result<Uuid, DbError> {
        self.guarded_unlink(id, "google_id", "google_id = $unset")
            .await
    }

    pub async fn unlink_steam(&self, id: Uuid) -> Result<Uuid, DbError> {
        self.guarded_unlink(id, "steam_id", "steam_id = $unset")
            .await
    }

    pub async fn unlink_facebook(&self, id: Uuid) -> Result<Uuid, DbError> {
        self.guarded_unlink(id, "facebook_id", "facebook_id = $unset")
            .await
    }

    pub async fn unlink_gamecenter(&self, id: Uuid) -> Result<Uuid, DbError> {
        self.guarded_unlink(id, "gamecenter_id", "gamecenter_id = $unset")
            .await
    }

    pub async fn unlink_facebook_instant(&self, id: Uuid) -> Result<Uuid, DbError> {
        self.guarded_unlink(
            id,
            "facebook_instant_game_id",
            "facebook_instant_game_id = $unset",
        )
        .await
    }

    pub async fn unlink_device(&self, id: &str, user_id: Uuid) -> Result<String, DbError> {
        let user: Users = self
            .one(
                "SELECT * FROM type::record('users', <uuid>$id) LIMIT 1",
                params!(id = user_id.to_string()),
            )
            .await?;
        let has_identity = identity_columns(&user).iter().any(|(_, present)| *present);
        let other_device: Option<UserDevice> = self
            .opt(
                "SELECT * FROM user_device WHERE user_id = $user_id \
                 AND id != type::record('user_device', $id) LIMIT 1",
                params!(user_id = user_id.to_string(), id = id),
            )
            .await?;
        if !has_identity && other_device.is_none() {
            return Err(DbError::NotFound);
        }
        let deleted: UserDevice = self
            .one(
                "DELETE type::record('user_device', $id) \
                 WHERE user_id = $user_id RETURN BEFORE",
                params!(id = id, user_id = user_id.to_string()),
            )
            .await?;
        Ok(deleted.id)
    }

    pub async fn unlink_provider(&self, provider: &str, user_id: Uuid) -> Result<String, DbError> {
        let user: Users = self
            .one(
                "SELECT * FROM type::record('users', <uuid>$id) LIMIT 1",
                params!(id = user_id.to_string()),
            )
            .await?;
        let has_identity = identity_columns(&user).iter().any(|(_, present)| *present);
        let other_device: Option<UserDevice> = self
            .opt(
                "SELECT * FROM user_device WHERE user_id = $user_id \
                 AND provider != $provider LIMIT 1",
                params!(user_id = user_id.to_string(), provider = provider),
            )
            .await?;
        if !has_identity && other_device.is_none() {
            return Err(DbError::NotFound);
        }
        let deleted: UserDevice = self
            .one(
                "DELETE user_device WHERE provider = $provider \
                 AND user_id = $user_id RETURN BEFORE",
                params!(provider = provider, user_id = user_id.to_string()),
            )
            .await?;
        Ok(deleted.id)
    }
}
