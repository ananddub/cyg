use super::*;

impl Queries {
    pub async fn link_email(
        &self,
        params: &LinkEmailParams,
    ) -> Result<Uuid, DbError> {
        update_returning_id(
            self,
            "UPDATE type::record('users', <uuid>$id) \
             SET email = $email, password = $password, update_time = $now RETURN AFTER",
            params!(
                email = params.email,
                password = bytes::encode(&params.d),
                now = time::encode(chrono::Utc::now()),
                id = params.id.to_string(),
            ),
        )
        .await
    }
    pub async fn link_custom_id(
        &self,
        custom_id: &str,
        id: Uuid,
    ) -> Result<Uuid, DbError> {
        update_returning_id(
            self,
            "UPDATE type::record('users', <uuid>$id) \
             SET custom_id = $custom_id, update_time = $now RETURN AFTER",
            params!(
                custom_id = custom_id,
                now = time::encode(chrono::Utc::now()),
                id = id.to_string(),
            ),
        )
        .await
    }
    pub async fn link_apple(
        &self,
        apple_id: &str,
        id: Uuid,
    ) -> Result<Uuid, DbError> {
        update_returning_id(
            self,
            "UPDATE type::record('users', <uuid>$id) \
             SET apple_id = $apple_id, update_time = $now RETURN AFTER",
            params!(
                apple_id = apple_id,
                now = time::encode(chrono::Utc::now()),
                id = id.to_string(),
            ),
        )
        .await
    }
    pub async fn link_google(
        &self,
        params: &LinkGoogleParams,
    ) -> Result<Uuid, DbError> {
        update_returning_id(
            self,
            "UPDATE type::record('users', <uuid>$id) SET \
                google_id = $google_id, \
                display_name = IF display_name IS NONE OR display_name IS NULL \
                    OR display_name = '' THEN $param3 ELSE display_name END, \
                avatar_url = IF avatar_url IS NONE OR avatar_url IS NULL \
                    OR avatar_url = '' THEN $param4 ELSE avatar_url END, \
                update_time = $now \
             RETURN AFTER",
            params!(
                google_id = params.google_id,
                param3 = params.param3,
                param4 = params.param4,
                now = time::encode(chrono::Utc::now()),
                id = params.id.to_string(),
            ),
        )
        .await
    }
    pub async fn link_steam(
        &self,
        steam_id: &str,
        id: Uuid,
    ) -> Result<Uuid, DbError> {
        update_returning_id(
            self,
            "UPDATE type::record('users', <uuid>$id) \
             SET steam_id = $steam_id, update_time = $now RETURN AFTER",
            params!(
                steam_id = steam_id,
                now = time::encode(chrono::Utc::now()),
                id = id.to_string(),
            ),
        )
        .await
    }
    pub async fn link_facebook(
        &self,
        params: &LinkFacebookParams,
    ) -> Result<Uuid, DbError> {
        update_returning_id(
            self,
            "UPDATE type::record('users', <uuid>$id) SET \
                facebook_id = $facebook_id, \
                display_name = IF display_name IS NONE OR display_name IS NULL \
                    OR display_name = '' THEN $param3 ELSE display_name END, \
                avatar_url = IF avatar_url IS NONE OR avatar_url IS NULL \
                    OR avatar_url = '' THEN $param4 ELSE avatar_url END, \
                update_time = $now \
             RETURN AFTER",
            params!(
                facebook_id = params.facebook_id,
                param3 = params.param3,
                param4 = params.param4,
                now = time::encode(chrono::Utc::now()),
                id = params.id.to_string(),
            ),
        )
        .await
    }
    pub async fn link_gamecenter(
        &self,
        gamecenter_id: &str,
        id: Uuid,
    ) -> Result<Uuid, DbError> {
        update_returning_id(
            self,
            "UPDATE type::record('users', <uuid>$id) \
             SET gamecenter_id = $gamecenter_id, update_time = $now RETURN AFTER",
            params!(
                gamecenter_id = gamecenter_id,
                now = time::encode(chrono::Utc::now()),
                id = id.to_string(),
            ),
        )
        .await
    }
}

impl Queries {
    pub async fn link_facebook_instant(
        &self,
        facebook_instant_game_id: &str,
        id: Uuid,
    ) -> Result<Uuid, DbError> {
        update_returning_id(
            self,
            "UPDATE type::record('users', <uuid>$id) \
             SET facebook_instant_game_id = $facebook_instant_game_id, \
                 update_time = $now RETURN AFTER",
            params!(
                facebook_instant_game_id = facebook_instant_game_id,
                now = time::encode(chrono::Utc::now()),
                id = id.to_string(),
            ),
        )
        .await
    }
    pub async fn import_email(
        &self,
        email: &str,
        id: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('users', <uuid>$id) SET email = $email",
            params!(email = email, id = id.to_string()),
        )
        .await
    }
    pub async fn import_email_if_empty(
        &self,
        email: &str,
        id: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('users', <uuid>$id) SET email = $email \
             WHERE email IS NONE OR email IS NULL OR email = ''",
            params!(email = email, id = id.to_string()),
        )
        .await
    }
    pub async fn count_owned_device(
        &self,
        id: &str,
        user_id: Uuid,
    ) -> Result<i64, DbError> {
        Ok(self
            .opt::<i64>(
                "RETURN array::len((SELECT id FROM user_device \
                 WHERE id = type::record('user_device', $id) \
                   AND user_id = $user_id))",
                params!(id = id, user_id = user_id.to_string()),
            )
            .await?
            .unwrap_or(0))
    }
    pub async fn upsert_provider_identity(
        &self,
        params: &UpsertProviderIdentityParams,
    ) -> Result<String, DbError> {
        let device = new_device(&params.id, params.user_id, &params.provider);
        match self
            .create("user_device", Key::String(params.id.clone()), &device)
            .await
        {
            Ok(_) => return Ok(params.id.clone()),
            Err(DbError::Unique(_)) => {}
            Err(err) => return Err(err),
        }
        let existing: UserDevice = self
            .one(
                "SELECT * FROM type::record('user_device', $id) LIMIT 1",
                params!(id = params.id),
            )
            .await?;
        if existing.user_id == params.user_id {
            Ok(params.id.clone())
        } else {
            Err(DbError::NotFound)
        }
    }
    pub async fn touch_user(
        &self,
        id: Uuid,
    ) -> Result<Uuid, DbError> {
        let user: Users = self
            .one(
                "UPDATE type::record('users', <uuid>$id) \
                 SET update_time = $now RETURN AFTER",
                params!(now = time::encode(chrono::Utc::now()), id = id.to_string()),
            )
            .await?;
        Ok(user.id)
    }
}
