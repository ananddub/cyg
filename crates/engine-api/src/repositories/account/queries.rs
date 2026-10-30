use super::*;

impl Queries {
    pub async fn update_account(&self, params_in: &UpdateAccountParams) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('users', <uuid>$id) SET \
                display_name = $display_name, avatar_url = $avatar_url, lang_tag = $lang_tag, \
                location = $location, timezone = $timezone, metadata = $metadata, \
                username = $username, update_time = $now",
            params!(
                display_name = params_in.display_name,
                avatar_url = params_in.avatar_url,
                lang_tag = params_in.lang_tag,
                location = params_in.location,
                timezone = params_in.timezone,
                metadata = params_in.metadata,
                username = params_in.username,
                id = params_in.id.to_string(),
                now = time::encode(chrono::Utc::now()),
            ),
        )
        .await
    }

    pub async fn set_custom_id(&self, id: Uuid, custom_id: &str) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('users', <uuid>$id) SET custom_id = $custom_id",
            params!(id = id.to_string(), custom_id = custom_id),
        )
        .await
    }

    pub async fn update_wallet_ledger_metadata(
        &self,
        user_id: Uuid,
        ledger_id: Uuid,
        metadata: serde_json::Value,
    ) -> Result<Vec<WalletLedger>, DbError> {
        self.all(
            "UPDATE wallet_ledger SET metadata = $metadata, update_time = $now \
             WHERE id = type::record('wallet_ledger', <uuid>$id) \
               AND user_id = $user_id RETURN BEFORE",
            params!(
                metadata = metadata,
                id = ledger_id.to_string(),
                user_id = user_id.to_string(),
                now = time::encode(chrono::Utc::now()),
            ),
        )
        .await
    }

    pub async fn delete_wallet_ledger(
        &self,
        ledger_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, DbError> {
        let deleted: Option<WalletLedger> = self
            .opt(
                "DELETE wallet_ledger WHERE id = type::record('wallet_ledger', <uuid>$id) \
                    AND user_id = $user_id RETURN BEFORE",
                params!(id = ledger_id.to_string(), user_id = user_id.to_string(),),
            )
            .await?;
        Ok(deleted.is_some())
    }

    pub async fn update_password(&self, d: &[u8], id: Uuid) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('users', <uuid>$id) SET password = $password, update_time = $now",
            params!(
                password = bytes::encode(d),
                id = id.to_string(),
                now = time::encode(chrono::Utc::now()),
            ),
        )
        .await
    }

    pub async fn delete_account(&self, id: Uuid) -> Result<(), DbError> {
        self.exec(
            "DELETE user_device WHERE user_id = $id; \
             DELETE message WHERE sender_id = $id; \
             DELETE notification WHERE user_id = $id; \
             DELETE users_notes WHERE user_id = $id; \
             DELETE wallet_ledger WHERE user_id = $id; \
             DELETE storage WHERE user_id = $id; \
             DELETE storage_index WHERE user_id = $id; \
             DELETE user_edge WHERE source_id = $id OR destination_id = $id; \
             DELETE type::record('users', <uuid>$id)",
            params!(id = id.to_string()),
        )
        .await
    }

    pub async fn create_tombstone(&self, user_id: Uuid) -> Result<(), DbError> {
        let row = UserTombstone {
            user_id,
            create_time: chrono::Utc::now(),
        };
        match self
            .create("user_tombstone", crate::db::Key::Uuid(user_id), &row)
            .await
        {
            Ok(_) | Err(DbError::Unique(_)) => Ok(()),
            Err(err) => Err(err),
        }
    }

    pub async fn list_user_devices(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<ListUserDevicesRow>, DbError> {
        self.all(
            "SELECT id, user_id, provider FROM user_device WHERE user_id = $user_id",
            params!(user_id = user_id.to_string()),
        )
        .await
    }

    pub async fn list_device_push_tokens(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<ListDevicePushTokensRow>, DbError> {
        self.all(
            "SELECT id, push_token_ios, push_token_android, push_token_amazon, \
                    push_token_huawei, push_token_web, provider \
             FROM user_device WHERE user_id = $user_id",
            params!(user_id = user_id.to_string()),
        )
        .await
    }

    pub async fn update_device_push_tokens(
        &self,
        tokens: &UpdateDevicePushTokensParams,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE user_device SET \
                push_token_ios = $push_token_ios, push_token_android = $push_token_android, \
                push_token_amazon = $push_token_amazon, push_token_huawei = $push_token_huawei, \
                push_token_web = $push_token_web \
             WHERE id = type::record('user_device', $id) AND user_id = $user_id",
            params!(
                push_token_ios = tokens.push_token_ios,
                push_token_android = tokens.roid,
                push_token_amazon = tokens.push_token_amazon,
                push_token_huawei = tokens.push_token_huawei,
                push_token_web = tokens.push_token_web,
                id = tokens.id,
                user_id = tokens.user_id.to_string(),
            ),
        )
        .await
    }
}
