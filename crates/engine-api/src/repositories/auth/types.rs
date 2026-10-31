use super::*;

pub type GetDeviceByIdRow = ListUserDevicesRow;

pub struct CreateUserWithCustomIdParams {
    pub id: Uuid,
    pub username: String,
    pub custom_id: String,
}

pub struct CreateUserWithEmailParams {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub password: Vec<u8>,
}

pub struct CreateUserWithAppleParams {
    pub id: Uuid,
    pub username: String,
    pub apple_id: String,
}

pub struct CreateUserWithFacebookParams {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_url: String,
    pub facebook_id: String,
}

pub struct CreateUserWithFacebookInstantParams {
    pub id: Uuid,
    pub username: String,
    pub facebook_instant_game_id: String,
}

pub struct CreateUserWithGamecenterParams {
    pub id: Uuid,
    pub username: String,
    pub gamecenter_id: String,
}

pub struct CreateUserWithGoogleParams {
    pub id: Uuid,
    pub username: String,
    pub google_id: String,
    pub display_name: String,
    pub avatar_url: String,
}

pub struct CreateUserWithSteamParams {
    pub id: Uuid,
    pub username: String,
    pub steam_id: String,
}

pub struct InsertProviderDeviceParams {
    pub provider: String,
    pub id: String,
    pub user_id: Uuid,
}

pub(super) fn new_user(id: Uuid, username: &str) -> Users {
    let now = Utc::now();
    Users {
        id,
        username: username.to_string(),
        display_name: None,
        avatar_url: None,
        lang_tag: "en".to_string(),
        location: None,
        timezone: None,
        metadata: json!({}),
        wallet: json!({}),
        email: None,
        password: None,
        facebook_id: None,
        google_id: None,
        gamecenter_id: None,
        steam_id: None,
        custom_id: None,
        edge_count: 0,
        create_time: now,
        update_time: now,
        verify_time: time::epoch(),
        disable_time: time::epoch(),
        facebook_instant_game_id: None,
        apple_id: None,
        game_id: None,
    }
}

pub(super) use crate::repositories::user::new_device;
