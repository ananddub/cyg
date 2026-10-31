use super::*;

impl Queries {
    pub async fn create_user_with_google(
        &self,
        p: &CreateUserWithGoogleParams,
    ) -> Result<Users, DbError> {
        let user = Users {
            google_id: Some(p.google_id.clone()),
            display_name: Some(p.display_name.clone()),
            avatar_url: Some(p.avatar_url.clone()),
            ..new_user(p.id, &p.username)
        };
        self.create("users", Key::Uuid(p.id), &user).await
    }
    pub async fn create_user_with_steam(
        &self,
        p: &CreateUserWithSteamParams,
    ) -> Result<Users, DbError> {
        let user = Users {
            steam_id: Some(p.steam_id.clone()),
            ..new_user(p.id, &p.username)
        };
        self.create("users", Key::Uuid(p.id), &user).await
    }
    pub async fn insert_device(&self, id: &str, user_id: Uuid) -> Result<String, DbError> {
        let device = new_device(id, user_id, "");
        self.create("user_device", Key::String(id.to_string()), &device)
            .await?;
        Ok(id.to_string())
    }
    pub async fn insert_provider_device(
        &self,
        p: &InsertProviderDeviceParams,
    ) -> Result<String, DbError> {
        let device = new_device(&p.id, p.user_id, &p.provider);
        self.create("user_device", Key::String(p.id.clone()), &device)
            .await?;
        Ok(p.id.clone())
    }
    pub async fn create_device_account(
        &self,
        device_id: &str,
        user: &Users,
    ) -> Result<bool, DbError> {
        let device = new_device(device_id, user.id, "");
        let mut user_value = serde_json::to_value(user)?;
        if let Value::Object(map) = &mut user_value {
            map.remove("id");
        }
        let created: Vec<Value> = self
            .all(
                "IF array::len((SELECT VALUE id FROM user_device \
                    WHERE id = type::record('user_device', $device_id))) = 0 { \
                     CREATE type::record('user_device', $device_id) CONTENT $device; \
                     CREATE type::record('users', <uuid>$id) CONTENT $user; \
                     RETURN true; \
                 } ELSE { RETURN false; };",
                params!(
                    device_id = device_id,
                    device = device,
                    id = user.id.to_string(),
                    user = user_value,
                ),
            )
            .await?;
        Ok(created.first().and_then(Value::as_bool).unwrap_or(false))
    }
    pub async fn delete_user(&self, id: Uuid) -> Result<(), DbError> {
        self.exec(
            "DELETE type::record('users', <uuid>$id)",
            params!(id = id.to_string()),
        )
        .await
    }
}

impl Queries {
    pub async fn get_user_by_id(&self, id: Uuid) -> Result<Users, DbError> {
        self.one(
            "SELECT * FROM type::record('users', <uuid>$id) LIMIT 1",
            params!(id = id.to_string()),
        )
        .await
    }
    pub async fn get_user_by_email(&self, email: &str) -> Result<Users, DbError> {
        self.one(
            "SELECT * FROM users WHERE email = $email LIMIT 1",
            params!(email = email),
        )
        .await
    }
    pub async fn get_user_by_username(&self, username: &str) -> Result<Users, DbError> {
        self.one(
            "SELECT * FROM users WHERE username = $username LIMIT 1",
            params!(username = username),
        )
        .await
    }
    pub async fn get_user_by_custom_id(&self, custom_id: &str) -> Result<Users, DbError> {
        self.one(
            "SELECT * FROM users WHERE custom_id = $custom_id LIMIT 1",
            params!(custom_id = custom_id),
        )
        .await
    }
    pub async fn get_user_by_facebook_id(&self, facebook_id: &str) -> Result<Users, DbError> {
        self.one(
            "SELECT * FROM users WHERE facebook_id = $facebook_id LIMIT 1",
            params!(facebook_id = facebook_id),
        )
        .await
    }
    pub async fn get_user_by_google_id(&self, google_id: &str) -> Result<Users, DbError> {
        self.one(
            "SELECT * FROM users WHERE google_id = $google_id LIMIT 1",
            params!(google_id = google_id),
        )
        .await
    }
    pub async fn get_user_by_apple_id(&self, apple_id: &str) -> Result<Users, DbError> {
        self.one(
            "SELECT * FROM users WHERE apple_id = $apple_id LIMIT 1",
            params!(apple_id = apple_id),
        )
        .await
    }
    pub async fn get_user_by_steam_id(&self, steam_id: &str) -> Result<Users, DbError> {
        self.one(
            "SELECT * FROM users WHERE steam_id = $steam_id LIMIT 1",
            params!(steam_id = steam_id),
        )
        .await
    }
    pub async fn get_user_by_gamecenter_id(&self, gamecenter_id: &str) -> Result<Users, DbError> {
        self.one(
            "SELECT * FROM users WHERE gamecenter_id = $gamecenter_id LIMIT 1",
            params!(gamecenter_id = gamecenter_id),
        )
        .await
    }
    pub async fn get_user_by_facebook_instant_game_id(
        &self,
        facebook_instant_game_id: &str,
    ) -> Result<Users, DbError> {
        self.one(
            "SELECT * FROM users WHERE facebook_instant_game_id = $facebook_instant_game_id LIMIT 1",
            params!(facebook_instant_game_id = facebook_instant_game_id),
        )
        .await
    }
    pub async fn get_device_by_id(&self, id: &str) -> Result<GetDeviceByIdRow, DbError> {
        self.one(
            "SELECT id, user_id, provider FROM user_device WHERE id = type::record('user_device', $id) LIMIT 1",
            params!(id = id),
        )
        .await
    }
    pub async fn create_user_bare(&self, id: Uuid, username: &str) -> Result<Users, DbError> {
        self.create("users", Key::Uuid(id), &new_user(id, username))
            .await
    }
    pub async fn create_user_with_custom_id(
        &self,
        p: &CreateUserWithCustomIdParams,
    ) -> Result<Users, DbError> {
        let user = Users {
            custom_id: Some(p.custom_id.clone()),
            ..new_user(p.id, &p.username)
        };
        self.create("users", Key::Uuid(p.id), &user).await
    }
    pub async fn create_user_with_email(
        &self,
        p: &CreateUserWithEmailParams,
    ) -> Result<Users, DbError> {
        let user = Users {
            email: Some(p.email.clone()),
            password: Some(p.password.clone()),
            ..new_user(p.id, &p.username)
        };
        self.create("users", Key::Uuid(p.id), &user).await
    }
    pub async fn create_user_with_apple(
        &self,
        p: &CreateUserWithAppleParams,
    ) -> Result<Users, DbError> {
        let user = Users {
            apple_id: Some(p.apple_id.clone()),
            ..new_user(p.id, &p.username)
        };
        self.create("users", Key::Uuid(p.id), &user).await
    }
    pub async fn create_user_with_facebook(
        &self,
        p: &CreateUserWithFacebookParams,
    ) -> Result<Users, DbError> {
        let user = Users {
            display_name: Some(p.display_name.clone()),
            avatar_url: Some(p.avatar_url.clone()),
            facebook_id: Some(p.facebook_id.clone()),
            ..new_user(p.id, &p.username)
        };
        self.create("users", Key::Uuid(p.id), &user).await
    }
    pub async fn create_user_with_facebook_instant(
        &self,
        p: &CreateUserWithFacebookInstantParams,
    ) -> Result<Users, DbError> {
        let user = Users {
            facebook_instant_game_id: Some(p.facebook_instant_game_id.clone()),
            ..new_user(p.id, &p.username)
        };
        self.create("users", Key::Uuid(p.id), &user).await
    }
    pub async fn create_user_with_gamecenter(
        &self,
        p: &CreateUserWithGamecenterParams,
    ) -> Result<Users, DbError> {
        let user = Users {
            gamecenter_id: Some(p.gamecenter_id.clone()),
            ..new_user(p.id, &p.username)
        };
        self.create("users", Key::Uuid(p.id), &user).await
    }
}
