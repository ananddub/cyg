use super::*;

pub(super) fn identity_columns(user: &Users) -> [(&'static str, bool); 8] {
    [
        ("email", user.email.is_some()),
        ("custom_id", user.custom_id.is_some()),
        ("apple_id", user.apple_id.is_some()),
        ("facebook_id", user.facebook_id.is_some()),
        ("facebook_instant_game_id", user.facebook_instant_game_id.is_some()),
        ("google_id", user.google_id.is_some()),
        ("gamecenter_id", user.gamecenter_id.is_some()),
        ("steam_id", user.steam_id.is_some()),
    ]
}
