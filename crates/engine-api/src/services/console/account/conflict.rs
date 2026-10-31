use crate::error::V2Error;

pub(super) fn import_conflict(e: crate::db::DbError) -> V2Error {
    let named = [
        ("users_pkey", "That account id already exists."),
        (
            "users_username_key",
            "An account with that username already exists.",
        ),
        (
            "users_email_key",
            "An account with that email already exists.",
        ),
        (
            "users_custom_id_key",
            "That custom id is already linked to another account.",
        ),
        (
            "users_apple_id_key",
            "That provider id is already linked to another account.",
        ),
        (
            "users_facebook_id_key",
            "That provider id is already linked to another account.",
        ),
        (
            "users_google_id_key",
            "That provider id is already linked to another account.",
        ),
        (
            "users_gamecenter_id_key",
            "That provider id is already linked to another account.",
        ),
        (
            "users_steam_id_key",
            "That provider id is already linked to another account.",
        ),
        (
            "users_facebook_instant_game_id_key",
            "That provider id is already linked to another account.",
        ),
        (
            "user_device_pkey",
            "One of that account's device ids already exists.",
        ),
    ];
    for (constraint, message) in named {
        if V2Error::is_unique_violation(&e, constraint) {
            return V2Error::already_exists(message);
        }
    }
    if V2Error::is_any_unique_violation(&e) {
        V2Error::already_exists("That account's id or one of its rows already exists.")
    } else {
        V2Error::from(e)
    }
}
