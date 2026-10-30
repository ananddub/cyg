use super::*;

pub(super) async fn create_device(
    state: &ApiState,
    device_id: &str,
    username: &str,
) -> Result<Uuid, V2Error> {
    let queries = q(state).await?;
    let id = Uuid::new_v4();
    let now = Utc::now();
    let user = Users {
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
    };
    let created = queries
        .create_device_account(device_id, &user)
        .await
        .map_err(insert_failed)?;
    if !created {
        return Err(V2Error::internal("Error finding or creating user account."));
    }
    Ok(id)
}
